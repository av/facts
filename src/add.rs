/// The `add` subcommand — append a fact to a file and section.
use std::path::Path;

use anyhow::{Context, Result};

use crate::id;
use crate::lock::FileLock;
use crate::model::{Fact, FactSheet, Section};
use crate::parser;
use crate::project;
use crate::writer;

/// Options for the add subcommand.
pub struct AddOptions {
    /// The fact label text.
    pub label: String,
    /// Target file (default: ".facts").
    pub file: Option<String>,
    /// Target section path (e.g. "cli/add"). Created if needed.
    pub section: Option<String>,
    /// Validation command.
    pub command: Option<String>,
    /// Explicit ID override.
    pub id: Option<String>,
    /// Tags to add.
    pub tags: Vec<String>,
}

/// Run the add subcommand (auto-detects project root).
/// Returns the assigned ID of the newly added fact.
pub fn run(opts: &AddOptions) -> Result<String> {
    let root = project::find_project_root()?;
    run_in(opts, &root)
}

/// Run the add subcommand in a given root directory.
/// Separated from `run` so tests can supply a temp dir without changing cwd.
/// Returns the assigned ID of the newly added fact.
fn run_in(opts: &AddOptions, root: &Path) -> Result<String> {
    if opts.label.contains('\n') || opts.label.contains('\r') {
        anyhow::bail!("label cannot contain newlines");
    }

    let (stripped_label, _) = parser::extract_inline_tags(&opts.label);
    if stripped_label.trim().is_empty() {
        anyhow::bail!("label cannot be empty");
    }

    if let Some(ref cmd) = opts.command
        && cmd.trim().is_empty()
    {
        anyhow::bail!("command cannot be empty");
    }

    if let Some(ref id) = opts.id
        && id.trim().is_empty()
    {
        anyhow::bail!("ID cannot be empty");
    }

    let filename = opts.file.as_deref().unwrap_or(".facts");

    if filename.starts_with('/') {
        anyhow::bail!("file path must be relative, not absolute");
    }

    if filename.contains("..") {
        anyhow::bail!("file path must not contain '..'");
    }

    let (filename, file_path) = project::resolve_file_arg(root, filename);

    let _lock = FileLock::acquire(root)?;

    if let Some(ref id) = opts.id {
        let all_files = project::discover_fact_files(root)?;
        for path in &all_files {
            if !path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(".facts");
            if let Ok(sheet) = parser::parse(&content, fname) {
                let already_exists = sheet
                    .all_facts()
                    .iter()
                    .any(|(_, f)| f.explicit_id.as_deref() == Some(id));
                if already_exists {
                    anyhow::bail!("ID already exists: {}", id);
                }
            }
        }
    }

    let mut sheet = if file_path.exists() {
        let content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("failed to read {}", file_path.display()))?;
        parser::parse(&content, &filename)?
    } else {
        FactSheet {
            filename: filename.clone(),
            preamble: Vec::new(),
            sections: Vec::new(),
        }
    };

    // Tags alone do NOT promote to mapping — they go inline as @tag.
    let needs_mapping = opts.command.is_some() || opts.id.is_some();
    let is_plain = !needs_mapping;

    let (clean_label, inline_tags) = parser::extract_inline_tags(&opts.label);

    let mut combined_tags = inline_tags;
    for t in &opts.tags {
        if !combined_tags.contains(t) {
            combined_tags.push(t.clone());
        }
    }

    let mut fact = Fact {
        explicit_id: opts.id.clone(),
        label: clean_label,
        command: opts.command.clone(),
        tags: combined_tags,
        is_plain,
        raw: String::new(),
        blank_lines_before: 0,
    };

    fact.raw = writer::fact_to_raw(&fact);

    let added_label = fact.label.clone();

    if let Some(ref section_path) = opts.section {
        add_to_section(&mut sheet, section_path, fact)?;
    } else {
        sheet.preamble.push(fact);
    }

    let output = writer::write(&sheet);
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    std::fs::write(&file_path, &output)
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    // Re-read all fact sheets and assign IDs to find the new fact's ID.
    let all_files = project::discover_with_explicit(root, opts.file.as_deref())?;
    let mut all_fact_labels: Vec<(String, Option<String>)> = Vec::new();
    let mut target_idx = None;

    for path in &all_files {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let fname = project::relative_filename(root, path);
        let file_sheet = parser::parse(&content, &fname)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let is_target_file = fname == filename;

        for (section_path, f) in file_sheet.all_facts() {
            let idx = all_fact_labels.len();
            all_fact_labels.push((f.label.clone(), f.explicit_id.clone()));

            // Match by label, file, and section path.
            if is_target_file && f.label == added_label {
                let fact_section = opts
                    .section
                    .as_ref()
                    .map(|s| {
                        s.split('/')
                            .map(|p| p.trim().to_string())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let matches_section = section_path
                    .iter()
                    .map(|s| s.to_lowercase())
                    .collect::<Vec<_>>()
                    == fact_section
                        .iter()
                        .map(|s| s.to_lowercase())
                        .collect::<Vec<_>>();
                if matches_section {
                    target_idx = Some(idx);
                }
            }
        }
    }

    let assigned_ids = id::assign_ids(&all_fact_labels);
    let idx = target_idx.ok_or_else(|| anyhow::anyhow!("failed to find newly added fact"))?;
    Ok(assigned_ids[idx].clone())
}

/// Add a fact to a section, creating the section path if needed.
fn add_to_section(sheet: &mut FactSheet, section_path: &str, fact: Fact) -> Result<()> {
    let parts: Vec<&str> = section_path.split('/').map(|p| p.trim()).collect();

    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        anyhow::bail!("section path cannot contain empty components");
    }

    // Markdown only supports headings up to level 6 (######).
    // Top-level sections start at depth 1, so at most 6 path components.
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

/// Parse a comma-separated tags string into a Vec<String>.
/// Returns an error if any tag contains whitespace.
pub fn parse_tags(tags_str: &str) -> Result<Vec<String>> {
    let tags: Vec<String> = tags_str
        .split(',')
        .map(|t| t.trim().trim_start_matches('@').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    for tag in &tags {
        if tag.contains(char::is_whitespace) {
            anyhow::bail!("tag '{}' cannot contain whitespace", tag);
        }
        if tag.contains('(') || tag.contains(')') {
            anyhow::bail!("tag '{}' cannot contain parentheses", tag);
        }
        if tag == "not" || tag == "and" || tag == "or" {
            anyhow::bail!("tag name conflicts with filter operator: {}", tag);
        }
    }
    Ok(tags)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup_test_dir() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let facts_path = dir.path().join(".facts");
        (dir, facts_path)
    }

    #[test]
    fn test_add_to_empty_file() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "a new fact".to_string(),
            file: None,
            section: None,
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert_eq!(content, "- a new fact\n");
    }

    #[test]
    fn test_add_to_existing_file() {
        let (dir, facts_path) = setup_test_dir();
        fs::write(&facts_path, "- existing fact\n").unwrap();

        let opts = AddOptions {
            label: "a new fact".to_string(),
            file: None,
            section: None,
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert_eq!(content, "- existing fact\n- a new fact\n");
    }

    #[test]
    fn test_add_with_command() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "project has cargo".to_string(),
            file: None,
            section: None,
            command: Some("test -f Cargo.toml".to_string()),
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert_eq!(
            content,
            "- label: project has cargo\n  command: test -f Cargo.toml\n"
        );
    }

    #[test]
    fn test_add_with_tags() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "a tagged fact".to_string(),
            file: None,
            section: None,
            command: None,
            id: None,
            tags: vec!["mvp".to_string(), "core".to_string()],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert_eq!(content, "- a tagged fact @mvp @core\n");
    }

    #[test]
    fn test_add_to_section() {
        let (dir, facts_path) = setup_test_dir();
        fs::write(&facts_path, "# project\n\n- some existing fact\n").unwrap();

        let opts = AddOptions {
            label: "a new section fact".to_string(),
            file: None,
            section: Some("project".to_string()),
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert!(content.contains("- some existing fact"));
        assert!(content.contains("- a new section fact"));
    }

    #[test]
    fn test_add_creates_section() {
        let (dir, facts_path) = setup_test_dir();
        fs::write(&facts_path, "- root fact\n").unwrap();

        let opts = AddOptions {
            label: "a section fact".to_string(),
            file: None,
            section: Some("new-section".to_string()),
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert!(content.contains("# new-section"));
        assert!(content.contains("- a section fact"));
    }

    #[test]
    fn test_add_creates_nested_section() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "nested fact".to_string(),
            file: None,
            section: Some("parent/child".to_string()),
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert!(content.contains("# parent"));
        assert!(content.contains("## child"));
        assert!(content.contains("- nested fact"));
    }

    #[test]
    fn test_add_creates_file() {
        let (dir, _) = setup_test_dir();
        let custom_path = dir.path().join("custom.facts");

        let opts = AddOptions {
            label: "custom file fact".to_string(),
            file: Some("custom.facts".to_string()),
            section: None,
            command: None,
            id: None,
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        assert!(custom_path.exists());
        let content = fs::read_to_string(&custom_path).unwrap();
        assert_eq!(content, "- custom file fact\n");
    }

    #[test]
    fn test_add_with_explicit_id() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "id fact".to_string(),
            file: None,
            section: None,
            command: None,
            id: Some("myid".to_string()),
            tags: vec![],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert!(content.contains("id: myid"));
        assert!(content.contains("label: id fact"));
    }

    #[test]
    fn test_parse_tags() {
        assert_eq!(parse_tags("mvp,core").unwrap(), vec!["mvp", "core"]);
        assert_eq!(parse_tags("mvp, core, ").unwrap(), vec!["mvp", "core"]);
        assert_eq!(parse_tags("single").unwrap(), vec!["single"]);
    }

    #[test]
    fn test_parse_tags_strips_at_prefix() {
        assert_eq!(parse_tags("@mvp,@core").unwrap(), vec!["mvp", "core"]);
        assert_eq!(parse_tags("@mvp,core").unwrap(), vec!["mvp", "core"]);
        assert_eq!(parse_tags("@mvp").unwrap(), vec!["mvp"]);
    }

    #[test]
    fn test_add_rejects_empty_id() {
        let (dir, _facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "some fact".to_string(),
            file: None,
            section: None,
            command: None,
            id: Some("".to_string()),
            tags: vec![],
        };
        let result = run_in(&opts, dir.path());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("ID cannot be empty")
        );
    }

    #[test]
    fn test_parse_tags_rejects_whitespace() {
        let err = parse_tags("has space").unwrap_err();
        assert!(
            err.to_string().contains("cannot contain whitespace"),
            "unexpected error: {err}"
        );

        let err = parse_tags("ok,has space,also_ok").unwrap_err();
        assert!(
            err.to_string().contains("'has space'"),
            "error should name the bad tag: {err}"
        );
    }

    #[test]
    fn test_parse_tags_rejects_parentheses() {
        let err = parse_tags("v(beta)").unwrap_err();
        assert!(
            err.to_string().contains("cannot contain parentheses"),
            "unexpected error: {err}"
        );

        let err = parse_tags("ok,v(beta),also_ok").unwrap_err();
        assert!(
            err.to_string().contains("'v(beta)'"),
            "error should name the bad tag: {err}"
        );
    }

    #[test]
    fn test_parse_tags_rejects_operator_names() {
        for op in &["not", "and", "or"] {
            let err = parse_tags(op).unwrap_err();
            assert!(
                err.to_string().contains("conflicts with filter operator"),
                "expected operator conflict error for '{op}', got: {err}"
            );
        }
        let err = parse_tags("ok,not,also_ok").unwrap_err();
        assert!(
            err.to_string().contains("not"),
            "error should mention the bad tag: {err}"
        );
    }

    #[test]
    fn test_add_all_options() {
        let (dir, facts_path) = setup_test_dir();

        let opts = AddOptions {
            label: "full fact".to_string(),
            file: None,
            section: Some("testing".to_string()),
            command: Some("echo ok".to_string()),
            id: Some("xyz".to_string()),
            tags: vec!["mvp".to_string()],
        };
        run_in(&opts, dir.path()).unwrap();

        let content = fs::read_to_string(&facts_path).unwrap();
        assert!(content.contains("# testing"));
        assert!(content.contains("label: full fact"));
        assert!(content.contains("id: xyz"));
        assert!(content.contains("command: echo ok"));
        assert!(content.contains("tags: [mvp]"));
    }
}
