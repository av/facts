/// The `check` subcommand — run command-facts and report pass/fail/manual.
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::color;
use crate::id;
use crate::model::FactSheet;
use crate::parser;
use crate::project;
use crate::tags::matches_tag_expr;

/// Options for the check command.
pub struct CheckOptions {
    pub tags_expr: Option<String>,
    pub timeout: Option<u64>,
}

/// Result of running a single command-fact.
struct CheckResult {
    id: String,
    display_path: String,
    status: CheckStatus,
}

enum CheckStatus {
    Passed { command: String },
    Failed { command: String, exit_code: i32, stderr: String },
    Manual,
}

/// Format the display path for a fact (file > section > label).
fn format_display_path(sheet: &FactSheet, section_path: &[String], label: &str) -> String {
    let file_prefix = sheet.display_name();
    let mut parts: Vec<&str> = Vec::new();
    if !file_prefix.is_empty() {
        parts.push(file_prefix);
    }
    for s in section_path {
        parts.push(s.as_str());
    }
    parts.push(label);
    parts.join(" > ")
}

/// Execute a command via $SHELL (fallback to sh) and return (exit_code, stderr).
fn run_command(command: &str, project_root: &Path, timeout: Option<Duration>) -> (i32, String) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());

    let mut cmd = Command::new(&shell);
    cmd.arg("-c")
        .arg(command)
        .current_dir(project_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());

    match timeout {
        Some(timeout) => run_with_timeout(cmd, timeout),
        None => run_no_timeout(cmd),
    }
}

fn run_no_timeout(mut cmd: Command) -> (i32, String) {
    match cmd.output() {
        Ok(output) => {
            let code = output.status.code().unwrap_or(1);
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            (code, stderr)
        }
        Err(e) => (1, format!("failed to run command: {e}")),
    }
}

fn run_with_timeout(mut cmd: Command, timeout: Duration) -> (i32, String) {
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (1, format!("failed to spawn command: {e}")),
    };

    let start = std::time::Instant::now();
    let poll_interval = Duration::from_millis(50);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process finished
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut e| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut e, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                return (status.code().unwrap_or(1), stderr);
            }
            Ok(None) => {
                // Still running — check timeout
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (124, "command timed out".to_string());
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return (1, format!("failed to wait on command: {e}"));
            }
        }
    }
}

/// Run the check subcommand.
pub fn run(opts: &CheckOptions) -> Result<bool> {
    let root = project::find_project_root()?;
    let files = project::discover_fact_files(&root)?;

    if files.is_empty() {
        eprintln!("no .facts files found in {}", root.display());
        return Ok(true);
    }

    let mut sheets = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".facts");
        let sheet = parser::parse(&content, filename)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        sheets.push(sheet);
    }

    // Collect all facts for ID assignment
    let mut all_fact_labels: Vec<(String, Option<String>)> = Vec::new();
    for sheet in &sheets {
        for (_path, fact) in sheet.all_facts() {
            all_fact_labels.push((fact.label.clone(), fact.explicit_id.clone()));
        }
    }
    let assigned_ids = id::assign_ids(&all_fact_labels);

    // Build list of facts to check (applying tag filter)
    let timeout = opts.timeout.map(Duration::from_secs);
    let mut results: Vec<CheckResult> = Vec::new();
    let mut fact_idx = 0;

    for sheet in &sheets {
        for (section_path, fact) in sheet.all_facts() {
            let id = assigned_ids[fact_idx].clone();
            fact_idx += 1;

            // Apply tag filter
            if let Some(ref expr) = opts.tags_expr {
                if !matches_tag_expr(expr, &fact.tags) {
                    continue;
                }
            }

            let display_path = format_display_path(sheet, &section_path, &fact.label);

            let status = match &fact.command {
                Some(command) => {
                    let (exit_code, stderr) = run_command(command, &root, timeout);
                    if exit_code == 0 {
                        CheckStatus::Passed { command: command.clone() }
                    } else {
                        CheckStatus::Failed {
                            command: command.clone(),
                            exit_code,
                            stderr: stderr.trim().to_string(),
                        }
                    }
                }
                None => CheckStatus::Manual,
            };

            results.push(CheckResult {
                id,
                display_path,
                status,
            });
        }
    }

    // Group by status
    let passed: Vec<&CheckResult> = results.iter().filter(|r| matches!(r.status, CheckStatus::Passed { .. })).collect();
    let failed: Vec<&CheckResult> = results.iter().filter(|r| matches!(r.status, CheckStatus::Failed { .. })).collect();
    let manual: Vec<&CheckResult> = results.iter().filter(|r| matches!(r.status, CheckStatus::Manual)).collect();

    let has_failures = !failed.is_empty();

    // Print passed
    if !passed.is_empty() {
        println!("{}", color::bold(&color::green("passed")));
        for r in &passed {
            if let CheckStatus::Passed { command } = &r.status {
                println!(
                    "  {} {} {}",
                    color::green(&format!("✓ {}", r.id)),
                    r.display_path,
                    color::dim(&format!("({command})")),
                );
            }
        }
        println!();
    }

    // Print failed
    if !failed.is_empty() {
        println!("{}", color::bold(&color::red("failed")));
        for r in &failed {
            if let CheckStatus::Failed { command, exit_code, stderr } = &r.status {
                println!(
                    "  {} {}",
                    color::red(&format!("✗ {}", r.id)),
                    r.display_path,
                );
                println!(
                    "         {} exit {}",
                    color::dim("command:"),
                    exit_code,
                );
                println!(
                    "         {} {command}",
                    color::dim("ran:"),
                );
                if !stderr.is_empty() {
                    for line in stderr.lines() {
                        println!(
                            "         {} {line}",
                            color::dim("stderr:"),
                        );
                    }
                }
            }
        }
        println!();
    }

    // Print manual
    if !manual.is_empty() {
        println!("{}", color::bold(&color::yellow("manual")));
        for r in &manual {
            println!(
                "  {} {}",
                color::yellow(&format!("? {}", r.id)),
                r.display_path,
            );
        }
        println!();
    }

    // Summary line
    let summary = format!(
        "{}, {}, {}",
        color::green(&format!("{} passed", passed.len())),
        color::red(&format!("{} failed", failed.len())),
        color::yellow(&format!("{} manual", manual.len())),
    );
    println!("{summary}");

    // Legend
    println!(
        "{}",
        color::dim("? = no command — verified manually by the agent"),
    );

    Ok(!has_failures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn project_root() -> PathBuf {
        project::find_project_root().unwrap()
    }

    #[test]
    fn test_run_command_success() {
        let (code, _stderr) = run_command("true", &project_root(), None);
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_command_failure() {
        let (code, _stderr) = run_command("false", &project_root(), None);
        assert_ne!(code, 0);
    }

    #[test]
    fn test_run_command_captures_stderr() {
        let (code, stderr) = run_command("echo 'oops' >&2; exit 1", &project_root(), None);
        assert_ne!(code, 0);
        assert!(stderr.contains("oops"));
    }

    #[test]
    fn test_run_command_exit_code() {
        let (code, _stderr) = run_command("exit 42", &project_root(), None);
        assert_eq!(code, 42);
    }

    #[test]
    fn test_run_command_uses_project_root_as_cwd() {
        let root = project_root();
        let (code, _stderr) = run_command("test -f Cargo.toml", &root, None);
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_command_timeout() {
        let (code, stderr) = run_command("sleep 10", &project_root(), Some(Duration::from_millis(100)));
        assert_eq!(code, 124);
        assert!(stderr.contains("timed out"));
    }

    #[test]
    fn test_format_display_path_default_file() {
        let sheet = FactSheet {
            filename: ".facts".to_string(),
            preamble: vec![],
            sections: vec![],
        };
        let path = vec!["cli".to_string(), "check".to_string()];
        let result = format_display_path(&sheet, &path, "runs commands");
        assert_eq!(result, "cli > check > runs commands");
    }

    #[test]
    fn test_format_display_path_named_file() {
        let sheet = FactSheet {
            filename: "cli.facts".to_string(),
            preamble: vec![],
            sections: vec![],
        };
        let path = vec!["check".to_string()];
        let result = format_display_path(&sheet, &path, "runs commands");
        assert_eq!(result, "cli.facts > check > runs commands");
    }

    #[test]
    fn test_format_display_path_no_section() {
        let sheet = FactSheet {
            filename: ".facts".to_string(),
            preamble: vec![],
            sections: vec![],
        };
        let result = format_display_path(&sheet, &[], "a preamble fact");
        assert_eq!(result, "a preamble fact");
    }
}
