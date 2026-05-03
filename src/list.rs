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
        validate_tag_expr(expr).map_err(|e| anyhow::anyhow!("invalid tag expression: {e}"))?;
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

    let id_width = assigned_ids.iter().map(|id| id.len()).max().unwrap_or(3);

    let mut fact_idx = 0;
    for sheet in &sheets {
        let file_matches = if let Some(ref f) = opts.file_filter {
            sheet.filename == *f || sheet.filename == format!("{f}.facts")
        } else {
            true
        };

        for (path, fact) in sheet.all_facts() {
            let id = &assigned_ids[fact_idx];
            fact_idx += 1;

            if !file_matches {
                continue;
            }

            // Apply section filter — case-insensitive path match (consistent
            // with `add --section` which uses eq_ignore_ascii_case).
            // --section "cli" matches "cli", "Cli", "cli/check", etc. but NOT "cli_tools".
            if let Some(ref section) = opts.section_filter {
                if !section_matches(&path.join("/"), section) {
                    continue;
                }
            }

            if opts.has_command && fact.command.is_none() {
                continue;
            }

            if opts.manual && fact.command.is_some() {
                continue;
            }

            if let Some(ref expr) = opts.tags_expr
                && !matches_tag_expr(expr, &fact.tags)
            {
                continue;
            }

            let display = format_fact_line(sheet, &path, id, &fact.label, &fact.tags, id_width);
            println!("{display}");
        }
    }

    Ok(())
}

/// Match a section filter against a path at any depth.
///
/// The filter is split by `/` into segments and matched as a contiguous
/// sub-path of the full section path. Children of matched sections are
/// included. Matching is case-insensitive.
///
/// Examples (path = "facts/cli/init"):
///   "init"       -> true  (single segment, found at depth 2)
///   "cli/init"   -> true  (two segments, found at depth 1-2)
///   "facts/cli"  -> true  (prefix match, also matches children)
///   "cli"        -> true  (matches cli and all children like cli/init)
///   "check"      -> false (not in this path)
///   "cli_tools"  -> false (must be exact segment match)
fn section_matches(path_str: &str, filter: &str) -> bool {
    let path_parts: Vec<&str> = path_str.split('/').collect();
    let filter_parts: Vec<&str> = filter.split('/').collect();

    if filter_parts.is_empty() || filter_parts.len() > path_parts.len() {
        return false;
    }

    for start in 0..=path_parts.len() - filter_parts.len() {
        if filter_parts
            .iter()
            .zip(&path_parts[start..])
            .all(|(f, p)| f.eq_ignore_ascii_case(p))
        {
            return true;
        }
    }

    false
}

/// Format a single fact line for display with color.
fn format_fact_line(
    sheet: &FactSheet,
    section_path: &[String],
    id: &str,
    label: &str,
    tags: &[String],
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

    let padded_id = format!("{:width$}", id, width = id_width);
    let dim_id = color::dim(&padded_id);
    let dim_sep = color::dim(">");

    let tag_suffix = if tags.is_empty() {
        String::new()
    } else {
        let tag_str = tags
            .iter()
            .map(|t| format!("@{t}"))
            .collect::<Vec<_>>()
            .join(" ");
        format!("  {}", color::dim(&tag_str))
    };

    if path_parts.is_empty() {
        format!("{dim_id}  {label}{tag_suffix}")
    } else {
        let colored_path = path_parts
            .iter()
            .map(|p| color::bold(p))
            .collect::<Vec<_>>()
            .join(&format!(" {dim_sep} "));
        format!("{dim_id}  {colored_path} {dim_sep} {label}{tag_suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_matches_exact() {
        assert!(section_matches("cli", "cli"));
        assert!(section_matches("cli/check", "cli/check"));
    }

    #[test]
    fn test_section_matches_prefix_includes_children() {
        assert!(section_matches("cli/check", "cli"));
        assert!(section_matches("cli/check/output", "cli"));
        assert!(section_matches("cli/check/output", "cli/check"));
    }

    #[test]
    fn test_section_matches_any_depth() {
        assert!(section_matches("facts/cli/init", "init"));
        assert!(section_matches("facts/cli/init", "cli/init"));
        assert!(section_matches("facts/cli/init", "cli"));
        assert!(section_matches("facts/skills/facts-discover", "skills"));
        assert!(section_matches(
            "facts/skills/facts-discover",
            "facts-discover"
        ));
    }

    #[test]
    fn test_section_matches_children_from_any_depth() {
        assert!(section_matches("facts/cli/init/extra", "init"));
        assert!(section_matches("facts/cli/init/extra", "cli/init"));
    }

    #[test]
    fn test_section_matches_case_insensitive() {
        assert!(section_matches("Api/Auth", "api/auth"));
        assert!(section_matches("facts/CLI/Init", "cli"));
        assert!(section_matches("facts/CLI/Init", "init"));
    }

    #[test]
    fn test_section_matches_no_substring() {
        assert!(!section_matches("cli_tools", "cli"));
        assert!(!section_matches("facts/cli_tools", "cli"));
        assert!(!section_matches("precli/check", "cli"));
    }

    #[test]
    fn test_section_matches_no_match() {
        assert!(!section_matches("facts/cli/init", "check"));
        assert!(!section_matches("facts/cli/init", "api"));
        assert!(!section_matches("cli", "cli/check"));
    }
}
