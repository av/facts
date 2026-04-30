/// The `add` subcommand — append a fact to a file and section.

use std::path::Path;

use anyhow::{Context, Result};

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
pub fn run(opts: &AddOptions) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(opts, &root)
}

/// Run the add subcommand in a given root directory.
/// Separated from `run` so tests can supply a temp dir without changing cwd.
fn run_in(opts: &AddOptions, root: &Path) -> Result<()> {
    if opts.label.contains('\n') || opts.label.contains('\r') {
        anyhow::bail!("label cannot contain newlines");
    }

    let filename = opts.file.as_deref().unwrap_or(".facts");

    // Ensure filename ends with .facts
    let filename = if filename.ends_with(".facts") {
        filename.to_string()
    } else {
        format!("{filename}.facts")
    };

    let file_path = root.join(&filename);

    // Parse existing file or create empty sheet
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

    // Determine if this should be a plain string or mapping fact.
    // Tags alone do NOT promote to mapping — they go inline as @tag.
    // Only command or explicit id require a mapping.
    let needs_mapping = opts.command.is_some() || opts.id.is_some();
    let is_plain = !needs_mapping;

    // Build the new fact
    let mut fact = Fact {
        explicit_id: opts.id.clone(),
        label: opts.label.clone(),
        command: opts.command.clone(),
        tags: opts.tags.clone(),
        is_plain,
        raw: String::new(),
        blank_lines_before: 0,
    };

    // Generate the raw representation
    fact.raw = writer::fact_to_raw(&fact);

    // Add the fact to the appropriate location
    if let Some(ref section_path) = opts.section {
        add_to_section(&mut sheet, section_path, fact)?;
    } else {
        // Add to preamble (root level)
        sheet.preamble.push(fact);
    }

    // Write back
    let output = writer::write(&sheet);
    std::fs::write(&file_path, &output)
        .with_context(|| format!("failed to write {}", file_path.display()))?;

    Ok(())
}

/// Add a fact to a section, creating the section path if needed.
fn add_to_section(sheet: &mut FactSheet, section_path: &str, fact: Fact) -> Result<()> {
    let parts: Vec<&str> = section_path.split('/').collect();

    if parts.is_empty() || parts.iter().any(|p| p.trim().is_empty()) {
        anyhow::bail!("section path cannot contain empty components");
    }

    // Navigate/create section hierarchy
    ensure_section_path(&mut sheet.sections, &parts, 1, fact);
    Ok(())
}

/// Recursively ensure the section path exists and append the fact to the leaf.
fn ensure_section_path(
    sections: &mut Vec<Section>,
    parts: &[&str],
    depth: usize,
    fact: Fact,
) {
    let target_name = parts[0];

    // Find existing section at this level
    let existing_idx = sections
        .iter()
        .position(|s| s.title.eq_ignore_ascii_case(target_name));

    if parts.len() == 1 {
        // This is the leaf section — add the fact here
        if let Some(idx) = existing_idx {
            let mut fact = fact;
            fact.blank_lines_before = 0;
            sections[idx].facts.push(fact);
        } else {
            // Create new section
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
        // Intermediate section — navigate deeper
        if let Some(idx) = existing_idx {
            ensure_section_path(&mut sections[idx].children, &parts[1..], depth + 1, fact);
        } else {
            // Create intermediate section and recurse
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
pub fn parse_tags(tags_str: &str) -> Vec<String> {
    tags_str
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
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
        // Tags alone do NOT promote to mapping — they stay inline as @tag
        assert_eq!(
            content,
            "- a tagged fact @mvp @core\n"
        );
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
        assert_eq!(parse_tags("mvp,core"), vec!["mvp", "core"]);
        assert_eq!(parse_tags("mvp, core, "), vec!["mvp", "core"]);
        assert_eq!(parse_tags("single"), vec!["single"]);
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
