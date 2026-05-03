/// The `fmt` subcommand — parse, validate, and normalize .facts files.
use anyhow::{Context, Result};

use crate::lint;
use crate::parser;
use crate::project;
use crate::writer;

/// Run the fmt subcommand.
pub fn run() -> Result<()> {
    let root = project::find_project_root()?;
    let files = project::discover_fact_files(&root)?;

    if files.is_empty() {
        eprintln!("no .facts files found in {}", root.display());
        return Ok(());
    }

    // Validate all files first — abort on any lint errors.
    let mut has_errors = false;
    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".facts");
        let diags = lint::lint_content(&content, filename);
        for diag in &diags {
            if diag.severity == lint::Severity::Error {
                let location = if let Some(line) = diag.line {
                    format!("{}:{}", diag.file, line)
                } else {
                    diag.file.clone()
                };
                eprintln!("{location}: error: {}", diag.message);
                has_errors = true;
            }
        }
    }
    if has_errors {
        anyhow::bail!("fmt aborted — fix lint errors first");
    }

    // Parse and write back each file.
    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".facts");
        let sheet = parser::parse(&content, filename)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        let output = writer::write(&sheet);
        if output != content {
            std::fs::write(path, &output)
                .with_context(|| format!("failed to write {}", path.display()))?;
        }
    }

    Ok(())
}
