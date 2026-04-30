/// The `lint` subcommand — validate that fact sheets are parseable.
///
/// Reports clear error messages for structural issues without running
/// any validation commands. Checks:
/// - Unparseable lines (not headings, facts, or blank)
/// - Invalid mapping keys (only id, label, command, tags are allowed)
/// - Mixed inline + mapping tags on the same fact
/// - Unparseable YAML in section bodies

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use std::collections::HashMap;

use crate::color;
use crate::model::FactSheet;
use crate::parser;
use crate::project;

/// Options for the lint command.
pub struct LintOptions {
    /// Lint a specific file instead of all *.facts files.
    pub file: Option<String>,
}

/// A single lint diagnostic.
#[derive(Debug)]
pub struct LintDiagnostic {
    pub file: String,
    pub line: Option<usize>,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

/// Run the lint subcommand. Returns true if all files pass.
pub fn run(opts: &LintOptions) -> Result<bool> {
    let root = project::find_project_root()?;
    let files = resolve_files(&root, opts)?;

    if files.is_empty() {
        eprintln!("no .facts files found in {}", root.display());
        return Ok(true);
    }

    let mut all_diagnostics: Vec<LintDiagnostic> = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".facts");

        let mut diags = lint_content(&content, filename);
        all_diagnostics.append(&mut diags);
    }

    if all_diagnostics.is_empty() {
        let count = files.len();
        let plural = if count == 1 { "" } else { "s" };
        println!(
            "{}",
            color::green(&format!("{count} file{plural} passed"))
        );
        return Ok(true);
    }

    // Print diagnostics
    for diag in &all_diagnostics {
        let severity_str = match diag.severity {
            Severity::Error => color::red("error"),
            Severity::Warning => color::yellow("warning"),
        };
        let location = if let Some(line) = diag.line {
            format!("{}:{}", diag.file, line)
        } else {
            diag.file.clone()
        };
        eprintln!("{}: {}: {}", color::bold(&location), severity_str, diag.message);
    }

    let error_count = all_diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warning_count = all_diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    let mut summary_parts = Vec::new();
    if error_count > 0 {
        summary_parts.push(format!(
            "{} error{}",
            error_count,
            if error_count == 1 { "" } else { "s" }
        ));
    }
    if warning_count > 0 {
        summary_parts.push(format!(
            "{} warning{}",
            warning_count,
            if warning_count == 1 { "" } else { "s" }
        ));
    }
    eprintln!("\n{}", summary_parts.join(", "));

    // Errors cause non-zero exit; warnings alone still pass
    Ok(error_count == 0)
}

/// Resolve which files to lint.
fn resolve_files(root: &Path, opts: &LintOptions) -> Result<Vec<PathBuf>> {
    if let Some(ref file) = opts.file {
        let path = root.join(file);
        if !path.exists() {
            anyhow::bail!("file not found: {}", path.display());
        }
        Ok(vec![path])
    } else {
        project::discover_fact_files(root)
    }
}

/// Lint a single file's content and return diagnostics.
pub fn lint_content(content: &str, filename: &str) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();

    // Check for CRLF line endings before any other checks.
    check_crlf(content, filename, &mut diagnostics);

    // Run all structural checks directly on the raw content.
    // These work independently of the parser so they catch issues even
    // if the parser would bail on the same content.
    check_line_structure(content, filename, &mut diagnostics);
    check_invalid_keys(content, filename, &mut diagnostics);
    check_mixed_tags(content, filename, &mut diagnostics);
    check_bare_tags(content, filename, &mut diagnostics);

    // Also try parsing; if the parser catches something our line-level
    // checks didn't, report that too. If parsing succeeds, run model-level
    // checks that need the parsed structure.
    if diagnostics.is_empty() {
        match parser::parse(content, filename) {
            Ok(sheet) => {
                check_duplicate_ids(&sheet, filename, &mut diagnostics);
            }
            Err(e) => {
                diagnostics.push(LintDiagnostic {
                    file: filename.to_string(),
                    line: None,
                    message: format!("failed to parse: {e}"),
                    severity: Severity::Error,
                });
            }
        }
    }

    diagnostics
}

/// Check if the file content contains CRLF line endings.
///
/// The facts writer always normalizes to LF, so CRLF input will not
/// round-trip byte-for-byte. Warn early so users know to convert.
fn check_crlf(content: &str, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    if content.contains("\r\n") {
        diagnostics.push(LintDiagnostic {
            file: filename.to_string(),
            line: None,
            message: "file uses CRLF line endings; facts normalizes to LF on write".to_string(),
            severity: Severity::Warning,
        });
    }
}

/// Check for mixed inline and mapping tags on the same fact.
fn check_mixed_tags(content: &str, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    let lines: Vec<&str> = content.lines().collect();
    let fact_groups = group_fact_lines(&lines);

    for group in fact_groups {
        if group.lines.len() < 2 {
            continue;
        }

        let mut has_inline_tags = false;
        let mut has_mapping_tags = false;

        // Check the label line for inline tags
        for line in &group.lines {
            let trimmed = line.trim();
            if trimmed.starts_with("- label: ") || trimmed.starts_with("label: ") {
                let val = if let Some(v) = trimmed.strip_prefix("- label: ") {
                    v
                } else if let Some(v) = trimmed.strip_prefix("label: ") {
                    v
                } else {
                    continue
                };
                // Check for @word tokens in label value
                for word in val.split_whitespace() {
                    if word.starts_with('@') && word.len() > 1 {
                        has_inline_tags = true;
                        break;
                    }
                }
            }
            let stripped = trimmed.strip_prefix("- ").unwrap_or(trimmed);
            if stripped.starts_with("tags: ") || stripped == "tags:" {
                has_mapping_tags = true;
            }
        }

        if has_inline_tags && has_mapping_tags {
            diagnostics.push(LintDiagnostic {
                file: filename.to_string(),
                line: Some(group.start_line),
                message: "mixed inline and mapping tags on the same fact".to_string(),
                severity: Severity::Warning,
            });
        }
    }
}

/// Check for `tags:` values that are not wrapped in brackets.
///
/// Bare comma-separated values like `tags: mvp, core` are silently ignored
/// by the parser. Warn users to use bracket syntax: `tags: [mvp, core]`.
fn check_bare_tags(content: &str, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    let lines: Vec<&str> = content.lines().collect();
    let fact_groups = group_fact_lines(&lines);

    for group in fact_groups {
        for (offset, line) in group.lines.iter().enumerate() {
            let trimmed = line.trim();
            let stripped = trimmed.strip_prefix("- ").unwrap_or(trimmed);

            if let Some(val) = stripped.strip_prefix("tags: ") {
                let val = val.trim();
                if !val.is_empty() && !(val.starts_with('[') && val.ends_with(']')) {
                    diagnostics.push(LintDiagnostic {
                        file: filename.to_string(),
                        line: Some(group.start_line + offset),
                        message: format!(
                            "tags should use bracket syntax: tags: [{}]",
                            val
                        ),
                        severity: Severity::Warning,
                    });
                }
            }
        }
    }
}

/// Check for duplicate explicit IDs across all facts in the sheet.
fn check_duplicate_ids(sheet: &FactSheet, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    let mut seen: HashMap<&str, usize> = HashMap::new();

    for (_path, fact) in sheet.all_facts() {
        if let Some(ref id) = fact.explicit_id {
            let count = seen.entry(id.as_str()).or_insert(0);
            *count += 1;
        }
    }

    for (id, count) in &seen {
        if *count > 1 {
            diagnostics.push(LintDiagnostic {
                file: filename.to_string(),
                line: None,
                message: format!(
                    "duplicate explicit id '{id}' appears {count} times"
                ),
                severity: Severity::Warning,
            });
        }
    }
}

/// Check for invalid mapping keys.
fn check_invalid_keys(content: &str, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    let lines: Vec<&str> = content.lines().collect();
    let fact_groups = group_fact_lines(&lines);
    let known_keys = ["label", "command", "id", "tags"];

    for group in fact_groups {
        // Only check multi-line facts (mappings)
        if group.lines.len() < 2 {
            // Also check single-line mapping-style facts
            let line = group.lines[0];
            let content_part = line.strip_prefix("- ").unwrap_or(line);
            if is_mapping_like(content_part) {
                check_keys_in_line(
                    content_part,
                    &known_keys,
                    filename,
                    group.start_line,
                    diagnostics,
                );
            }
            continue;
        }

        // First line: strip "- " prefix
        let first = group.lines[0].strip_prefix("- ").unwrap_or(group.lines[0]);
        check_keys_in_line(first, &known_keys, filename, group.start_line, diagnostics);

        // Continuation lines
        for (offset, line) in group.lines[1..].iter().enumerate() {
            let trimmed = line.trim();
            check_keys_in_line(
                trimmed,
                &known_keys,
                filename,
                group.start_line + offset + 1,
                diagnostics,
            );
        }
    }
}

/// Check a single key: value line for unknown keys.
fn check_keys_in_line(
    line: &str,
    known_keys: &[&str],
    filename: &str,
    line_num: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Some(colon_pos) = line.find(": ") {
        let key = &line[..colon_pos];
        // Only flag if the key looks like a simple identifier (no spaces)
        if !key.contains(' ') && !key.is_empty() && !known_keys.contains(&key) {
            diagnostics.push(LintDiagnostic {
                file: filename.to_string(),
                line: Some(line_num),
                message: format!("unknown key '{key}' in fact mapping (allowed: id, label, command, tags)"),
                severity: Severity::Error,
            });
        }
    }
}

/// Check that every non-blank line is either a heading or a fact line.
fn check_line_structure(content: &str, filename: &str, diagnostics: &mut Vec<LintDiagnostic>) {
    for (i, line) in content.lines().enumerate() {
        let line_num = i + 1;

        // Blank lines are fine
        if line.trim().is_empty() {
            continue;
        }

        // Headings are fine
        if line.trim_start().starts_with('#') {
            continue;
        }

        // Fact start (- prefixed)
        if line.starts_with("- ") || line == "-" {
            continue;
        }

        // Continuation of a mapping (indented)
        if line.starts_with("  ") {
            continue;
        }

        // Anything else is a structural error
        diagnostics.push(LintDiagnostic {
            file: filename.to_string(),
            line: Some(line_num),
            message: format!("unexpected line (not a heading, fact, or blank): {}", line.trim()),
            severity: Severity::Error,
        });
    }
}

/// A group of lines forming a single fact.
struct FactLineGroup<'a> {
    lines: Vec<&'a str>,
    start_line: usize, // 1-indexed
}

/// Group content lines into fact entries (each starting with `- `).
fn group_fact_lines<'a>(lines: &[&'a str]) -> Vec<FactLineGroup<'a>> {
    let mut groups: Vec<FactLineGroup<'a>> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1;

        // Skip blank lines and headings
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }

        if line.starts_with("- ") || *line == "-" {
            groups.push(FactLineGroup {
                lines: vec![line],
                start_line: line_num,
            });
        } else if line.starts_with("  ") && !groups.is_empty() {
            // Continuation of the previous fact
            groups.last_mut().unwrap().lines.push(line);
        }
    }

    groups
}

/// Check if a single-line content (after `- `) looks like a mapping.
fn is_mapping_like(content: &str) -> bool {
    let known_keys = ["label:", "command:", "id:", "tags:"];
    for key in &known_keys {
        if content.starts_with(key) {
            return true;
        }
    }
    if content.starts_with('{') && content.ends_with('}') {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_valid_file() {
        let content = "\
# project

- a fact about the project
- another fact

## section

- label: a mapping fact
  command: echo hi
";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn test_lint_valid_plain_with_tags() {
        let content = "- a fact @mvp @core\n";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn test_lint_valid_mapping_with_tags_key() {
        let content = "- label: a fact\n  tags: [mvp, core]\n";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn test_lint_catches_mixed_tags() {
        let content = "- label: a fact @mvp\n  tags: [core]\n";
        let diags = lint_content(content, ".facts");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert!(diags[0].message.contains("mixed inline and mapping tags"));
    }

    #[test]
    fn test_lint_catches_invalid_key() {
        let content = "- label: a fact\n  priority: high\n";
        let diags = lint_content(content, ".facts");
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(!errors.is_empty(), "expected error for unknown key");
        assert!(errors[0].message.contains("unknown key 'priority'"));
    }

    #[test]
    fn test_lint_catches_unexpected_line() {
        let content = "# title\n\nthis is not a fact\n- a real fact\n";
        let diags = lint_content(content, ".facts");
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(!errors.is_empty(), "expected error for unexpected line");
        assert!(errors[0].message.contains("unexpected line"));
    }

    #[test]
    fn test_lint_allows_known_keys() {
        let content = "\
- label: my fact
  id: abc
  command: echo ok
  tags: [core]
";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty(), "expected no diagnostics, got: {diags:?}");
    }

    #[test]
    fn test_lint_real_facts_file() {
        // The project's own .facts file should pass lint
        let content = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".facts"),
        )
        .unwrap();
        let diags = lint_content(&content, ".facts");
        // Filter to only errors (the .facts spec uses "both can coexist" so no warnings expected either)
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
        assert!(errors.is_empty(), "expected no errors on project .facts, got: {errors:?}");
    }

    #[test]
    fn test_lint_empty_file() {
        let content = "";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty());
    }

    #[test]
    fn test_lint_multiple_invalid_keys() {
        let content = "\
- label: fact one
  priority: high
  status: active
";
        let diags = lint_content(content, ".facts");
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Error).collect();
        assert_eq!(errors.len(), 2, "expected 2 errors for 2 unknown keys");
    }

    #[test]
    fn test_lint_catches_duplicate_ids() {
        let content = "\
- label: first fact
  id: dupe
- label: second fact
  id: dupe
";
        let diags = lint_content(content, ".facts");
        let warnings: Vec<_> = diags.iter().filter(|d| d.severity == Severity::Warning).collect();
        assert_eq!(warnings.len(), 1, "expected 1 warning for duplicate id");
        assert!(warnings[0].message.contains("duplicate"));
        assert!(warnings[0].message.contains("dupe"));
    }

    #[test]
    fn test_lint_unique_ids_pass() {
        let content = "\
- label: first fact
  id: alpha
- label: second fact
  id: beta
";
        let diags = lint_content(content, ".facts");
        assert!(diags.is_empty(), "expected no diagnostics for unique ids, got: {diags:?}");
    }

    #[test]
    fn test_lint_warns_bare_tags() {
        let content = "- label: my fact\n  tags: mvp, core\n";
        let diags = lint_content(content, ".facts");
        let warnings: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == Severity::Warning)
            .collect();
        assert_eq!(warnings.len(), 1, "expected 1 warning for bare tags");
        assert!(warnings[0].message.contains("tags should use bracket syntax"));
        assert!(warnings[0].message.contains("[mvp, core]"));
    }

    #[test]
    fn test_lint_bracket_tags_pass() {
        let content = "- label: my fact\n  tags: [mvp, core]\n";
        let diags = lint_content(content, ".facts");
        assert!(
            diags.is_empty(),
            "expected no diagnostics for bracket tags, got: {diags:?}"
        );
    }

    #[test]
    fn test_check_crlf_warns_on_crlf() {
        let mut diagnostics = Vec::new();
        check_crlf("- a fact\r\n- another\r\n", ".facts", &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Severity::Warning);
        assert!(diagnostics[0].message.contains("CRLF"));
        assert!(diagnostics[0].message.contains("LF"));
    }

    #[test]
    fn test_check_crlf_passes_on_lf() {
        let mut diagnostics = Vec::new();
        check_crlf("- a fact\n- another\n", ".facts", &mut diagnostics);
        assert!(diagnostics.is_empty(), "expected no CRLF warning for LF content");
    }
}
