/// Write-back serializer for .facts files.
///
/// Produces byte-for-byte deterministic output: parse then write with no
/// changes yields identical content.

use crate::model::{Fact, FactSheet, Section};

/// Serialize a FactSheet back to the .facts format.
pub fn write(sheet: &FactSheet) -> String {
    let mut out = String::new();

    // Preamble facts (before any heading)
    write_facts(&mut out, &sheet.preamble, true);

    // Sections
    for section in &sheet.sections {
        write_section(&mut out, section);
    }

    // Ensure file ends with a trailing newline
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }

    out
}

/// Write a section and its children recursively.
fn write_section(out: &mut String, section: &Section) {
    // Blank lines before heading
    for _ in 0..section.blank_lines_before {
        out.push('\n');
    }
    out.push_str(&section.raw_heading);
    out.push('\n');

    // Facts in this section
    write_facts(out, &section.facts, false);

    // Child sections
    for child in &section.children {
        write_section(out, child);
    }
}

/// Write a list of facts.
fn write_facts(out: &mut String, facts: &[Fact], is_preamble: bool) {
    for (i, fact) in facts.iter().enumerate() {
        // Blank lines before fact
        let blanks = if is_preamble && i == 0 && fact.blank_lines_before == 0 {
            0
        } else {
            fact.blank_lines_before
        };
        for _ in 0..blanks {
            out.push('\n');
        }
        out.push_str(&fact.raw);
        out.push('\n');
    }
}

/// Serialize a single new fact to its raw text representation.
/// This is used when creating facts via the `add` command.
pub fn fact_to_raw(fact: &Fact) -> String {
    if fact.is_plain {
        // Plain string fact
        let mut line = format!("- {}", fact.label);
        // Tags go inline for plain string facts
        for tag in &fact.tags {
            line.push_str(&format!(" @{tag}"));
        }
        line
    } else {
        // Mapping fact
        let mut lines = Vec::new();
        if let Some(ref id) = fact.explicit_id {
            lines.push(format!("  id: {id}"));
        }
        lines.insert(0, format!("- label: {}", fact.label));
        if let Some(ref cmd) = fact.command {
            lines.push(format!("  command: {cmd}"));
        }
        if !fact.tags.is_empty() {
            let tag_list = fact.tags.join(", ");
            lines.push(format!("  tags: [{tag_list}]"));
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    #[test]
    fn test_roundtrip_simple() {
        let input = "- a simple fact\n";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_roundtrip_with_sections() {
        let input = "# title\n\n- fact one\n\n## sub\n\n- fact two\n";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_roundtrip_mapping_fact() {
        let input = "- label: project is a Cargo project\n  command: test -f Cargo.toml\n";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_roundtrip_preamble_and_sections() {
        let input = "\
# facts

- a CLI for fact-driven development with coding agents

## format

### file
- project root is the directory containing the nearest parent .git
- fact sheets are *.facts files in the project root
";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_roundtrip_full_facts_file() {
        let content = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".facts"),
        )
        .unwrap();
        let sheet = parser::parse(&content, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, content);
    }

    #[test]
    fn test_roundtrip_tags_inline() {
        let input = "- a tagged fact @mvp @core\n";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_roundtrip_mapping_with_tags() {
        let input = "- label: some fact\n  command: echo hi\n  tags: [mvp, core]\n";
        let sheet = parser::parse(input, ".facts").unwrap();
        let output = write(&sheet);
        assert_eq!(output, input);
    }

    #[test]
    fn test_fact_to_raw_plain() {
        let fact = Fact {
            explicit_id: None,
            label: "a simple fact".to_string(),
            command: None,
            tags: vec![],
            is_plain: true,
            raw: String::new(),
            blank_lines_before: 0,
        };
        assert_eq!(fact_to_raw(&fact), "- a simple fact");
    }

    #[test]
    fn test_fact_to_raw_plain_with_tags() {
        let fact = Fact {
            explicit_id: None,
            label: "a tagged fact".to_string(),
            command: None,
            tags: vec!["mvp".to_string(), "core".to_string()],
            is_plain: true,
            raw: String::new(),
            blank_lines_before: 0,
        };
        assert_eq!(fact_to_raw(&fact), "- a tagged fact @mvp @core");
    }

    #[test]
    fn test_fact_to_raw_mapping() {
        let fact = Fact {
            explicit_id: None,
            label: "check cargo".to_string(),
            command: Some("test -f Cargo.toml".to_string()),
            tags: vec![],
            is_plain: false,
            raw: String::new(),
            blank_lines_before: 0,
        };
        assert_eq!(
            fact_to_raw(&fact),
            "- label: check cargo\n  command: test -f Cargo.toml"
        );
    }

    #[test]
    fn test_fact_to_raw_mapping_with_all_fields() {
        let fact = Fact {
            explicit_id: Some("xyz".to_string()),
            label: "full fact".to_string(),
            command: Some("echo ok".to_string()),
            tags: vec!["mvp".to_string()],
            is_plain: false,
            raw: String::new(),
            blank_lines_before: 0,
        };
        assert_eq!(
            fact_to_raw(&fact),
            "- label: full fact\n  id: xyz\n  command: echo ok\n  tags: [mvp]"
        );
    }
}
