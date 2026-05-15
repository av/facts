/// The `edit` subcommand — modify a fact by ID.
use std::path::Path;

use anyhow::{Context, Result};

use crate::id;
use crate::locate::{self, FactLocation};
use crate::lock::FileLock;
use crate::model::{Fact, FactSheet};
use crate::parser;
use crate::project;
use crate::writer;

/// Options for the edit subcommand.
pub struct EditOptions {
    /// The ID(s) of the fact(s) to edit.
    pub target_ids: Vec<String>,
    /// New label (replaces existing).
    pub label: Option<String>,
    /// New command (replaces existing).
    pub command: Option<String>,
    /// New explicit ID (replaces existing).
    pub new_id: Option<String>,
    /// New tags (replaces all existing).
    pub tags: Option<Vec<String>>,
    /// Tags to add (appended to existing, deduplicated).
    pub add_tags: Option<Vec<String>>,
    /// Tags to remove.
    pub remove_tags: Option<Vec<String>>,
}

/// Run the edit subcommand (auto-detects project root).
pub fn run(opts: &EditOptions) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(opts, &root)
}

/// Run the edit subcommand in a given root directory.
pub fn run_in(opts: &EditOptions, root: &Path) -> Result<()> {
    if opts.target_ids.is_empty() {
        anyhow::bail!("at least one ID is required");
    }

    if opts.target_ids.len() > 1 {
        if opts.new_id.is_some() {
            anyhow::bail!("--new-id cannot be used with multiple IDs");
        }
        if opts.label.is_some() {
            anyhow::bail!("--label cannot be used with multiple IDs");
        }
    }

    if let Some(ref label) = opts.label {
        if label.contains('\n') || label.contains('\r') {
            anyhow::bail!("label cannot contain newlines");
        }
        let (stripped_label, _) = parser::extract_inline_tags(label);
        if stripped_label.trim().is_empty() {
            anyhow::bail!("label cannot be empty");
        }
    }

    if let Some(ref cmd) = opts.command
        && cmd.trim().is_empty()
    {
        anyhow::bail!("command cannot be empty");
    }

    if let Some(ref new_id) = opts.new_id
        && new_id.trim().is_empty()
    {
        anyhow::bail!("ID cannot be empty");
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

    // Resolve all target IDs before making any changes (fail-all-before-writing).
    let mut resolved: Vec<usize> = Vec::new();
    for target_id in &opts.target_ids {
        let match_idx = assigned_ids
            .iter()
            .position(|id| id == target_id)
            .ok_or_else(|| anyhow::anyhow!("no fact found with ID '{}'", target_id))?;
        resolved.push(match_idx);
    }

    // Reject duplicate --new-id: check that the new ID doesn't already exist
    // among OTHER facts (not the one being edited).
    if let Some(ref new_id) = opts.new_id {
        for (i, (_, explicit_id)) in all_fact_labels.iter().enumerate() {
            if !resolved.contains(&i) && explicit_id.as_deref() == Some(new_id.as_str()) {
                anyhow::bail!("ID already exists: {}", new_id);
            }
        }
    }

    // Apply edits to all resolved facts.
    for &match_idx in &resolved {
        let (sheet_idx, ref location) = fact_locations[match_idx];
        let (_, ref mut sheet) = sheets[sheet_idx];

        let fact = locate::get_fact_mut(sheet, location);
        apply_edits(fact, opts);

        let fact = locate::get_fact_mut(sheet, location);
        fact.raw = writer::fact_to_raw(fact);
    }

    // Write back only files that were modified.
    let modified_sheets: std::collections::HashSet<usize> =
        resolved.iter().map(|&idx| fact_locations[idx].0).collect();

    for sheet_idx in modified_sheets {
        let (ref file_path, ref sheet) = sheets[sheet_idx];
        let output = writer::write(sheet);
        std::fs::write(file_path, &output)
            .with_context(|| format!("failed to write {}", file_path.display()))?;
    }

    for id in &opts.target_ids {
        println!("{}", id);
    }

    Ok(())
}

/// Apply edits to a fact based on the options.
fn apply_edits(fact: &mut Fact, opts: &EditOptions) {
    let original_explicit_id = fact.explicit_id.clone();

    if let Some(ref new_label) = opts.label {
        let (clean_label, inline_tags) = crate::parser::extract_inline_tags(new_label);
        fact.label = clean_label;
        for t in inline_tags {
            if !fact.tags.contains(&t) {
                fact.tags.push(t);
            }
        }
    }

    if let Some(ref new_command) = opts.command {
        fact.command = Some(new_command.clone());
    }

    if let Some(ref new_id) = opts.new_id {
        fact.explicit_id = Some(new_id.clone());
    } else {
        // Preserve existing explicit ID when editing other fields
        fact.explicit_id = original_explicit_id;
    }

    if let Some(ref new_tags) = opts.tags {
        fact.tags = new_tags.clone();
    }

    if let Some(ref tags_to_add) = opts.add_tags {
        for tag in tags_to_add {
            if !fact.tags.contains(tag) {
                fact.tags.push(tag.clone());
            }
        }
    }

    if let Some(ref tags_to_remove) = opts.remove_tags {
        fact.tags.retain(|t| !tags_to_remove.contains(t));
    }

    // Tags alone do NOT promote — they stay inline as @tag for plain facts.
    if fact.is_plain {
        let needs_mapping = fact.command.is_some() || fact.explicit_id.is_some();
        if needs_mapping {
            fact.is_plain = false;
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
    fn test_edit_label() {
        let content = "- original label\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "original label");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: Some("new label".to_string()),
            command: None,
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("new label"));
        assert!(!result.contains("original label"));
    }

    #[test]
    fn test_edit_command() {
        let content = "- label: test fact\n  command: echo old\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "test fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: Some("echo new".to_string()),
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("echo new"));
        assert!(!result.contains("echo old"));
    }

    #[test]
    fn test_edit_tags() {
        let content = "- label: tagged fact\n  tags: [old]\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "tagged fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: None,
            tags: Some(vec!["new1".to_string(), "new2".to_string()]),
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("tags: [new1, new2]"));
        assert!(!result.contains("old"));
    }

    #[test]
    fn test_add_tag() {
        let content = "- label: tagged fact\n  tags: [existing]\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "tagged fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: None,
            tags: None,
            add_tags: Some(vec!["new".to_string()]),
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("tags: [existing, new]"));
    }

    #[test]
    fn test_add_tag_deduplicates() {
        let content = "- label: tagged fact\n  tags: [existing]\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "tagged fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: None,
            tags: None,
            add_tags: Some(vec!["existing".to_string()]),
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("tags: [existing]"));
    }

    #[test]
    fn test_remove_tag() {
        let content = "- label: tagged fact\n  tags: [keep, remove]\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "tagged fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: Some(vec!["remove".to_string()]),
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("tags: [keep]"));
        assert!(!result.contains("remove"));
    }

    #[test]
    fn test_add_tag_to_plain_fact() {
        let content = "- a plain fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "a plain fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: None,
            tags: None,
            add_tags: Some(vec!["implemented".to_string()]),
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("a plain fact @implemented"));
    }

    #[test]
    fn test_edit_plain_to_mapping_with_command() {
        let content = "- a plain fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "a plain fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: Some("echo check".to_string()),
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("label: a plain fact"));
        assert!(result.contains("command: echo check"));
        assert!(!result.starts_with("- a plain fact\n"));
    }

    #[test]
    fn test_edit_plain_to_mapping_with_id() {
        let content = "- a plain fact\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "a plain fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: None,
            new_id: Some("myid".to_string()),
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("label: a plain fact"));
        assert!(result.contains("id: myid"));
    }

    #[test]
    fn test_edit_preserves_explicit_id() {
        let content = "- label: has id\n  id: keep-me\n  command: echo old\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = "keep-me".to_string();
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: Some("changed label".to_string()),
            command: None,
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("label: changed label"));
        assert!(result.contains("id: keep-me"));
        assert!(result.contains("command: echo old"));
    }

    #[test]
    fn test_edit_unknown_id_errors() {
        let content = "- some fact\n";
        let (dir, _) = setup_test_dir(content);

        let opts = EditOptions {
            target_ids: vec!["zzz".to_string()],
            label: Some("new".to_string()),
            command: None,
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        let result = run_in(&opts, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("zzz"), "error should mention the ID: {err}");
    }

    #[test]
    fn test_edit_tags_migrate_inline_to_mapping() {
        let content = "- a tagged fact @mvp @core\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "a tagged fact");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: None,
            command: Some("echo check".to_string()),
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("label: a tagged fact"));
        assert!(result.contains("command: echo check"));
        assert!(result.contains("tags: [mvp, core]"));
        assert!(!result.contains("@mvp"));
        assert!(!result.contains("@core"));
    }

    #[test]
    fn test_edit_in_section() {
        let content = "# section\n\n- fact in section\n";
        let (dir, facts_path) = setup_test_dir(content);

        let target_id = find_id_for_label(content, "fact in section");
        let opts = EditOptions {
            target_ids: vec![target_id],
            label: Some("edited fact".to_string()),
            command: None,
            new_id: None,
            tags: None,
            add_tags: None,
            remove_tags: None,
        };
        run_in(&opts, dir.path()).unwrap();

        let result = fs::read_to_string(&facts_path).unwrap();
        assert!(result.contains("# section"));
        assert!(result.contains("edited fact"));
        assert!(!result.contains("fact in section"));
    }
}
