/// The `move` subcommand — relocate a fact by ID to a different section or file.
use std::path::Path;

use anyhow::{Context, Result};

use crate::id;
use crate::locate::{self, FactLocation};
use crate::lock::FileLock;
use crate::model::{Fact, FactSheet, Section};
use crate::parser;
use crate::project;
use crate::writer;

/// Options for the move subcommand.
pub struct MoveOptions {
    /// The ID of the fact to move.
    pub target_id: String,
    /// Target section path (e.g. "cli/check"). Created if needed.
    pub target_section: Option<String>,
    /// Target .facts file (e.g. "api.facts").
    pub target_file: Option<String>,
}

/// Run the move subcommand (auto-detects project root).
pub fn run(opts: &MoveOptions) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(opts, &root)
}

/// Run the move subcommand in a given root directory.
pub fn run_in(opts: &MoveOptions, root: &Path) -> Result<()> {
    // At least one of --section or --file must be provided.
    if opts.target_section.is_none() && opts.target_file.is_none() {
        anyhow::bail!("must provide at least one of --section or --file");
    }

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

    // Build ID map across all sheets.
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
        .position(|id| *id == opts.target_id)
        .ok_or_else(|| anyhow::anyhow!("no fact found with ID '{}'", opts.target_id))?;

    let (source_sheet_idx, ref location) = fact_locations[match_idx];

    // Extract the fact (clone its properties, not the raw representation).
    let source_fact = locate::get_fact(&sheets[source_sheet_idx].1, location);
    let mut moved_fact = Fact {
        explicit_id: source_fact.explicit_id.clone(),
        label: source_fact.label.clone(),
        command: source_fact.command.clone(),
        tags: source_fact.tags.clone(),
        is_plain: source_fact.is_plain,
        raw: String::new(),
        blank_lines_before: 0,
    };
    moved_fact.raw = writer::fact_to_raw(&moved_fact);

    // Determine the target file.
    let target_filename = if let Some(ref f) = opts.target_file {
        if f.ends_with(".facts") {
            f.clone()
        } else {
            format!("{f}.facts")
        }
    } else {
        // Same file as source.
        sheets[source_sheet_idx].1.filename.clone()
    };

    // Find target sheet index (or mark for creation).
    let target_sheet_idx = sheets.iter().position(|(path, _)| {
        path.file_name()
            .and_then(|n| n.to_str())
            .map(|n| n == target_filename)
            .unwrap_or(false)
    });

    let is_cross_file = match target_sheet_idx {
        Some(idx) => idx != source_sheet_idx,
        None => true, // New file = cross-file.
    };

    // Remove the fact from its source location.
    remove_fact(&mut sheets[source_sheet_idx].1, location);

    // Add the fact to the target location.
    if let Some(target_idx) = target_sheet_idx {
        // Target file already exists in our sheets.
        if let Some(ref section_path) = opts.target_section {
            add_to_section(&mut sheets[target_idx].1, section_path, moved_fact)?;
        } else {
            sheets[target_idx].1.preamble.push(moved_fact);
        }
    } else {
        // Target file doesn't exist yet — create a new sheet.
        let mut new_sheet = FactSheet {
            filename: target_filename.clone(),
            preamble: Vec::new(),
            sections: Vec::new(),
        };
        if let Some(ref section_path) = opts.target_section {
            add_to_section(&mut new_sheet, section_path, moved_fact)?;
        } else {
            new_sheet.preamble.push(moved_fact);
        }
        let target_path = root.join(&target_filename);
        sheets.push((target_path, new_sheet));
    }

    // Write the source file.
    let (ref source_path, ref source_sheet) = sheets[source_sheet_idx];
    let source_output = writer::write(source_sheet);
    if source_output.trim().is_empty() {
        std::fs::remove_file(source_path)
            .with_context(|| format!("failed to remove {}", source_path.display()))?;
    } else {
        std::fs::write(source_path, &source_output)
            .with_context(|| format!("failed to write {}", source_path.display()))?;
    }

    // Write the target file if it's different from the source.
    if is_cross_file {
        let target_idx = sheets
            .iter()
            .position(|(path, _)| {
                path.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == target_filename)
                    .unwrap_or(false)
            })
            .unwrap();
        let (ref target_path, ref target_sheet) = sheets[target_idx];
        let target_output = writer::write(target_sheet);
        std::fs::write(target_path, &target_output)
            .with_context(|| format!("failed to write {}", target_path.display()))?;
    }

    println!("{}", assigned_ids[match_idx]);

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

/// Add a fact to a section, creating the section path if needed.
fn add_to_section(sheet: &mut FactSheet, section_path: &str, fact: Fact) -> Result<()> {
    let parts: Vec<&str> = section_path.split('/').map(|p| p.trim()).collect();

    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        anyhow::bail!("section path cannot contain empty components");
    }

    if parts.len() > 6 {
        anyhow::bail!("section path too deep (max 6 levels)");
    }

    ensure_section_path(&mut sheet.sections, &parts, 1, fact);
    Ok(())
}

/// Recursively ensure the section path exists and append the fact to the leaf.
fn ensure_section_path(sections: &mut Vec<Section>, parts: &[&str], depth: usize, fact: Fact) {
    let target_name = parts[0];

    let existing_idx = sections
        .iter()
        .position(|s| s.title.eq_ignore_ascii_case(target_name));

    if parts.len() == 1 {
        if let Some(idx) = existing_idx {
            let mut fact = fact;
            fact.blank_lines_before = 0;
            sections[idx].facts.push(fact);
        } else {
            let mut fact = fact;
            fact.blank_lines_before = 0;
            let section = Section {
                title: target_name.to_string(),
                depth,
                facts: vec![fact],
                children: Vec::new(),
                raw_heading: format!("{} {}", "#".repeat(depth), target_name),
                blank_lines_before: 1,
            };
            sections.push(section);
        }
    } else {
        if let Some(idx) = existing_idx {
            ensure_section_path(&mut sections[idx].children, &parts[1..], depth + 1, fact);
        } else {
            let mut section = Section {
                title: target_name.to_string(),
                depth,
                facts: Vec::new(),
                children: Vec::new(),
                raw_heading: format!("{} {}", "#".repeat(depth), target_name),
                blank_lines_before: 1,
            };
            ensure_section_path(&mut section.children, &parts[1..], depth + 1, fact);
            sections.push(section);
        }
    }
}

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
    fn test_move_to_section_same_file() {
        let content = "# alpha\n\n- fact to move\n\n# beta\n\n- existing beta fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to move");
        let opts = MoveOptions {
            target_id,
            target_section: Some("beta".to_string()),
            target_file: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(
            !result.contains("# alpha"),
            "empty source section should be removed"
        );
        assert!(result.contains("# beta"));
        assert!(result.contains("fact to move"));
        assert!(result.contains("existing beta fact"));
    }

    #[test]
    fn test_move_to_different_file() {
        let content = "- fact to move\n- fact to stay\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to move");
        let opts = MoveOptions {
            target_id,
            target_section: None,
            target_file: Some("other.facts".to_string()),
        };
        run_in(&opts, dir.path()).unwrap();

        let source = fs::read_to_string(&facts_path).unwrap();
        assert!(!source.contains("fact to move"));
        assert!(source.contains("fact to stay"));

        let target = fs::read_to_string(dir.path().join("other.facts")).unwrap();
        assert!(target.contains("fact to move"));
    }

    #[test]
    fn test_move_creates_target_section() {
        let content = "- fact to move\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to move");
        let opts = MoveOptions {
            target_id,
            target_section: Some("new/nested".to_string()),
            target_file: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("# new"));
        assert!(result.contains("## nested"));
        assert!(result.contains("fact to move"));
    }

    #[test]
    fn test_move_cleans_up_empty_sections() {
        let content = "# only\n\n- fact to move\n\n# keep\n\n- keep fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to move");
        let opts = MoveOptions {
            target_id,
            target_section: Some("keep".to_string()),
            target_file: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(
            !result.contains("# only"),
            "empty section should be cleaned up"
        );
        assert!(result.contains("# keep"));
        assert!(result.contains("fact to move"));
    }

    #[test]
    fn test_move_retains_properties() {
        let content = "- label: full fact\n  id: myid\n  command: echo ok\n  tags: [mvp, core]\n";
        let (dir, facts_path) = setup_test_dir(content);

        let opts = MoveOptions {
            target_id: "myid".to_string(),
            target_section: Some("target".to_string()),
            target_file: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("label: full fact"));
        assert!(result.contains("id: myid"));
        assert!(result.contains("command: echo ok"));
        assert!(result.contains("tags: [mvp, core]"));
        assert!(result.contains("# target"));
    }

    #[test]
    fn test_move_requires_section_or_file() {
        let content = "- some fact\n";
        let (dir, _) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "some fact");
        let opts = MoveOptions {
            target_id,
            target_section: None,
            target_file: None,
        };
        let result = run_in(&opts, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("at least one"), "error: {err}");
    }

    #[test]
    fn test_move_unknown_id_errors() {
        let content = "- some fact\n";
        let (dir, _) = setup_test_dir(content);

        let opts = MoveOptions {
            target_id: "zzz".to_string(),
            target_section: Some("dest".to_string()),
            target_file: None,
        };
        let result = run_in(&opts, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("zzz"), "error should mention the ID: {err}");
    }

    #[test]
    fn test_move_cross_file_with_section() {
        let content = "- fact to move\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact to move");
        let opts = MoveOptions {
            target_id,
            target_section: Some("api/endpoints".to_string()),
            target_file: Some("api.facts".to_string()),
        };
        run_in(&opts, dir.path()).unwrap();

        let source = fs::read_to_string(&facts_path).unwrap();
        assert!(!source.contains("fact to move"));

        let target = fs::read_to_string(dir.path().join("api.facts")).unwrap();
        assert!(target.contains("# api"));
        assert!(target.contains("## endpoints"));
        assert!(target.contains("fact to move"));
    }
}
