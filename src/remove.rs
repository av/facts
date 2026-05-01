/// The `remove` subcommand — remove a fact by ID.
use std::path::Path;

use anyhow::{Context, Result};

use crate::id;
use crate::locate::{self, FactLocation};
use crate::lock::FileLock;
use crate::model::FactSheet;
use crate::parser;
use crate::project;
use crate::writer;

/// Run the remove subcommand (auto-detects project root).
pub fn run(target_id: &str) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(target_id, &root)
}

/// Run the remove subcommand in a given root directory.
pub fn run_in(target_id: &str, root: &Path) -> Result<()> {
    let _lock = FileLock::acquire(root)?;

    let files = project::discover_fact_files(root)?;

    if files.is_empty() {
        anyhow::bail!("no .facts files found in {}", root.display());
    }

    let mut sheets: Vec<(std::path::PathBuf, FactSheet)> = Vec::new();
    for path in &files {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(".facts");
        let sheet = parser::parse(&content, filename)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        sheets.push((path.clone(), sheet));
    }

    let mut all_fact_labels: Vec<(String, Option<String>)> = Vec::new();
    let mut fact_locations: Vec<(usize, FactLocation)> = Vec::new();

    for (sheet_idx, (_, sheet)) in sheets.iter().enumerate() {
        for (fact_idx, fact) in sheet.preamble.iter().enumerate() {
            all_fact_labels.push((fact.label.clone(), fact.explicit_id.clone()));
            fact_locations.push((sheet_idx, FactLocation::Preamble(fact_idx)));
        }
        locate::collect_section_locations(
            sheet_idx,
            &sheet.sections,
            &[],
            &mut all_fact_labels,
            &mut fact_locations,
        );
    }

    let assigned_ids = id::assign_ids(&all_fact_labels);

    let match_idx = assigned_ids
        .iter()
        .position(|id| id == target_id)
        .ok_or_else(|| anyhow::anyhow!("no fact found with ID '{target_id}'"))?;

    let (sheet_idx, ref location) = fact_locations[match_idx];
    let (ref file_path, ref mut sheet) = sheets[sheet_idx];

    let removed_label = locate::get_fact(sheet, location).label.clone();
    remove_fact(sheet, location);

    let output = writer::write(sheet);
    if output.trim().is_empty() {
        std::fs::write(file_path, "")
            .with_context(|| format!("failed to write {}", file_path.display()))?;
    } else {
        std::fs::write(file_path, &output)
            .with_context(|| format!("failed to write {}", file_path.display()))?;
    }

    println!("{removed_label}");

    Ok(())
}

/// Remove a fact at the given location. Also removes empty sections.
fn remove_fact(sheet: &mut FactSheet, location: &FactLocation) {
    match location {
        FactLocation::Preamble(idx) => {
            sheet.preamble.remove(*idx);
        }
        FactLocation::Section(path, fact_idx) => {
            {
                let section = locate::navigate_to_section_mut(&mut sheet.sections, path);
                section.facts.remove(*fact_idx);
            }
            cleanup_empty_sections(&mut sheet.sections, path);
        }
    }
}

/// Remove sections that have no facts and no children, walking from leaf to root.
fn cleanup_empty_sections(sections: &mut Vec<Section>, path: &[usize]) {
    for depth in (0..path.len()).rev() {
        let current_path = &path[..=depth];
        let section = locate::navigate_to_section(sections, current_path);
        if section.facts.is_empty() && section.children.is_empty() {
            if depth == 0 {
                sections.remove(current_path[0]);
            } else {
                let parent_path = &current_path[..current_path.len() - 1];
                let parent = locate::navigate_to_section_mut(sections, parent_path);
                parent.children.remove(current_path[depth]);
            }
        } else {
            break;
        }
    }
}

use crate::model::Section;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir(content: &str) -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let facts_path = dir.path().join(".facts");
        fs::write(&facts_path, content).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        (dir, facts_path)
    }

    /// Helper: parse, assign IDs, find the ID for a label.
    fn find_id_for_label(content: &str, label: &str) -> String {
        let sheet = parser::parse(content, ".facts").unwrap();
        let all_facts = sheet.all_facts();
        let labels: Vec<(String, Option<String>)> = all_facts
            .iter()
            .map(|(_, f)| (f.label.clone(), f.explicit_id.clone()))
            .collect();
        let ids = id::assign_ids(&labels);
        for (i, (_, fact)) in all_facts.iter().enumerate() {
            if fact.label == label {
                return ids[i].clone();
            }
        }
        panic!("label '{label}' not found");
    }

    #[test]
    fn test_remove_fact_by_id() {
        let content = "- fact one\n- fact two\n- fact three\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact two");
        run_in(&target_id, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("fact one"));
        assert!(!result.contains("fact two"));
        assert!(result.contains("fact three"));
    }

    #[test]
    fn test_remove_outputs_label() {
        let content = "- fact to remove\n";
        let (dir, _) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to remove");
        run_in(&target_id, dir.path()).unwrap();
    }

    #[test]
    fn test_remove_last_fact_removes_section() {
        let content = "# mysection\n\n- only fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "only fact");
        run_in(&target_id, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(!result.contains("mysection"));
        assert!(!result.contains("only fact"));
    }

    #[test]
    fn test_remove_preserves_other_sections() {
        let content = "# section-a\n\n- fact a\n\n# section-b\n\n- fact b1\n- fact b2\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact a");
        run_in(&target_id, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(!result.contains("section-a"));
        assert!(!result.contains("fact a"));
        assert!(result.contains("section-b"));
        assert!(result.contains("fact b1"));
        assert!(result.contains("fact b2"));
    }

    #[test]
    fn test_remove_unknown_id_errors() {
        let content = "- some fact\n";
        let (dir, _) = setup_test_dir(content);

        let result = run_in("zzz", dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("zzz"), "error should mention the ID: {err}");
    }

    #[test]
    fn test_remove_nested_section_cleanup() {
        let content = "# parent\n\n## child\n\n- only child fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "only child fact");
        run_in(&target_id, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(!result.contains("parent"));
        assert!(!result.contains("child"));
    }

    #[test]
    fn test_remove_child_keeps_parent_with_facts() {
        let content = "# parent\n\n- parent fact\n\n## child\n\n- child fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "child fact");
        run_in(&target_id, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("parent"));
        assert!(result.contains("parent fact"));
        assert!(!result.contains("child"));
        assert!(!result.contains("child fact"));
    }
}
