/// The `list` subcommand — show facts in file order.
use anyhow::{Context, Result};

use crate::color;
use crate::id;
use crate::model::FactSheet;
use crate::parser;
use crate::project;
use crate::tags::{matches_tag_expr, validate_tag_expr};

/// Options for the list command.
pub struct ListOptions {
    pub file_filter: Option<String>,
    pub section_filter: Option<String>,
    pub has_command: bool,
    pub manual: bool,
    pub tags_expr: Option<String>,
}

/// Run the list subcommand.
pub fn run(opts: &ListOptions) -> Result<()> {
    // Validate tag expression up front so malformed expressions fail early
    // instead of silently producing empty output.
    if let Some(ref expr) = opts.tags_expr {
        validate_tag_expr(expr)
            .map_err(|e| anyhow::anyhow!("invalid tag expression: {e}"))?;
    }

    let root = project::find_project_root()?;
    let files = project::discover_fact_files(&root)?;

    if files.is_empty() {
        eprintln!("no .facts files found in {}", root.display());
        return Ok(());
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

    // Collect ALL facts across ALL sheets for globally-unique ID assignment.
    // IDs must be computed before any filtering so they stay stable regardless
    // of --file / --section / --tags flags.
    let mut all_fact_labels: Vec<(String, Option<String>)> = Vec::new();
    for sheet in &sheets {
        for (_path, fact) in sheet.all_facts() {
            all_fact_labels.push((fact.label.clone(), fact.explicit_id.clone()));
        }
    }

    let assigned_ids = id::assign_ids(&all_fact_labels);

    // Compute the maximum ID width for alignment.
    let id_width = assigned_ids.iter().map(|id| id.len()).max().unwrap_or(3);

    // Display facts, applying filters post-ID-assignment
    let mut fact_idx = 0;
    for sheet in &sheets {
        // Apply file filter
        let file_matches = if let Some(ref f) = opts.file_filter {
            sheet.filename == *f || sheet.filename == format!("{f}.facts")
        } else {
            true
        };

        for (path, fact) in sheet.all_facts() {
            let id = &assigned_ids[fact_idx];
            fact_idx += 1;

            // Skip entire file if filtered out
            if !file_matches {
                continue;
            }

            // Apply section filter — exact path match.
            // --section "cli" matches "cli", "cli/check", etc. but NOT "cli_tools".
            if let Some(ref section) = opts.section_filter {
                let path_str = path.join("/");
                if path_str != *section && !path_str.starts_with(&format!("{section}/")) {
                    continue;
                }
            }

            if opts.has_command && fact.command.is_none() {
                continue;
            }

            if opts.manual && fact.command.is_some() {
                continue;
            }

            if let Some(ref expr) = opts.tags_expr {
                if !matches_tag_expr(expr, &fact.tags) {
                    continue;
                }
            }

            // Format output line
            let display = format_fact_line(sheet, &path, id, &fact.label, id_width);
            println!("{display}");
        }
    }

    Ok(())
}

/// Format a single fact line for display with color.
fn format_fact_line(
    sheet: &FactSheet,
    section_path: &[String],
    id: &str,
    label: &str,
    id_width: usize,
) -> String {
    let file_prefix = sheet.display_name();
    let path_parts: Vec<&str> = if file_prefix.is_empty() {
        section_path.iter().map(|s| s.as_str()).collect()
    } else {
        let mut parts = vec![file_prefix];
        parts.extend(section_path.iter().map(|s| s.as_str()));
        parts
    };

    // Right-pad the ID so all content after it aligns.
    let padded_id = format!("{:width$}", id, width = id_width);
    let dim_id = color::dim(&padded_id);
    let dim_sep = color::dim(">");

    if path_parts.is_empty() {
        format!("{dim_id}  {label}")
    } else {
        let colored_path = path_parts
            .iter()
            .map(|p| color::bold(p))
            .collect::<Vec<_>>()
            .join(&format!(" {dim_sep} "));
        format!("{dim_id}  {colored_path} {dim_sep} {label}")
    }
}
