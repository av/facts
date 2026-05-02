/// The `get` subcommand — look up a single fact by ID and display its details.
use std::path::Path;

use anyhow::{Context, Result};

use crate::id;
use crate::locate::{self, FactLocation};
use crate::model::FactSheet;
use crate::parser;
use crate::project;

/// Run the get subcommand (auto-detects project root).
pub fn run(target_id: &str) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(target_id, &root)
}

/// Run the get subcommand in a given root directory.
pub fn run_in(target_id: &str, root: &Path) -> Result<()> {
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
    let mut fact_section_paths: Vec<Vec<String>> = Vec::new();

    for (sheet_idx, (_, sheet)) in sheets.iter().enumerate() {
        for (fact_idx, fact) in sheet.preamble.iter().enumerate() {
            all_fact_labels.push((fact.label.clone(), fact.explicit_id.clone()));
            fact_locations.push((sheet_idx, FactLocation::Preamble(fact_idx)));
            fact_section_paths.push(vec![]);
        }
        collect_section_paths(
            sheet_idx,
            &sheet.sections,
            &[],
            &[],
            &mut all_fact_labels,
            &mut fact_locations,
            &mut fact_section_paths,
        );
    }

    let assigned_ids = id::assign_ids(&all_fact_labels);

    let match_idx = assigned_ids
        .iter()
        .position(|id| id == target_id)
        .ok_or_else(|| anyhow::anyhow!("no fact found with ID '{target_id}'"))?;

    let (sheet_idx, ref location) = fact_locations[match_idx];
    let (_, ref sheet) = sheets[sheet_idx];
    let fact = locate::get_fact(sheet, location);
    let section_path = &fact_section_paths[match_idx];

    // Print label (always present)
    println!("label: {}", fact.label);

    // Print section path (omit for preamble facts)
    if !section_path.is_empty() {
        println!("section: {}", section_path.join("/"));
    }

    // Print file (always — but use display_name which is empty for .facts)
    let file_display = &sheet.filename;
    println!("file: {file_display}");

    // Print explicit ID if different from computed
    if let Some(ref explicit_id) = fact.explicit_id {
        println!("id: {explicit_id}");
    }

    // Print command if present
    if let Some(ref command) = fact.command {
        println!("command: {command}");
    }

    // Print tags if present
    if !fact.tags.is_empty() {
        let tag_list = fact.tags.join(", ");
        println!("tags: [{tag_list}]");
    }

    Ok(())
}

/// Recursively collect fact locations and section name paths from sections.
fn collect_section_paths(
    sheet_idx: usize,
    sections: &[crate::model::Section],
    parent_indices: &[usize],
    parent_names: &[String],
    all_labels: &mut Vec<(String, Option<String>)>,
    locations: &mut Vec<(usize, FactLocation)>,
    section_paths: &mut Vec<Vec<String>>,
) {
    for (sec_idx, section) in sections.iter().enumerate() {
        let mut idx_path = parent_indices.to_vec();
        idx_path.push(sec_idx);
        let mut name_path = parent_names.to_vec();
        name_path.push(section.title.clone());
        for (fact_idx, fact) in section.facts.iter().enumerate() {
            all_labels.push((fact.label.clone(), fact.explicit_id.clone()));
            locations.push((sheet_idx, FactLocation::Section(idx_path.clone(), fact_idx)));
            section_paths.push(name_path.clone());
        }
        collect_section_paths(
            sheet_idx,
            &section.children,
            &idx_path,
            &name_path,
            all_labels,
            locations,
            section_paths,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let facts_path = dir.path().join(".facts");
        fs::write(&facts_path, content).unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        dir
    }

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
    fn test_get_plain_fact() {
        let content = "- a plain fact\n";
        let dir = setup_test_dir(content);
        let target_id = find_id_for_label(content, "a plain fact");
        run_in(&target_id, dir.path()).unwrap();
    }

    #[test]
    fn test_get_fact_with_command() {
        let content = "- label: test fact\n  command: echo hello\n";
        let dir = setup_test_dir(content);
        let target_id = find_id_for_label(content, "test fact");
        run_in(&target_id, dir.path()).unwrap();
    }

    #[test]
    fn test_get_unknown_id_errors() {
        let content = "- some fact\n";
        let dir = setup_test_dir(content);
        let result = run_in("zzz", dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("zzz"), "error should mention the ID: {err}");
    }

    #[test]
    fn test_get_fact_in_section() {
        let content = "# mysection\n\n- fact in section\n";
        let dir = setup_test_dir(content);
        let target_id = find_id_for_label(content, "fact in section");
        run_in(&target_id, dir.path()).unwrap();
    }
}
