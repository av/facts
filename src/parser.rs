/// Parser for .facts files.
///
/// Strategy: extract headings as structure first, then parse each section's
/// body lines as YAML sequence items.
use anyhow::{Context, Result, bail};

use crate::model::{Fact, FactSheet, Section};

/// Parse a .facts file from its content and filename.
pub fn parse(content: &str, filename: &str) -> Result<FactSheet> {
    // Strip UTF-8 BOM if present (editors like Notepad may prepend it).
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);
    let lines: Vec<&str> = content.lines().collect();
    let blocks = split_into_blocks(&lines);

    let mut preamble = Vec::new();
    let mut sections = Vec::new();

    for block in &blocks {
        match block {
            Block::Preamble(fact_lines) => {
                preamble = parse_facts(fact_lines)
                    .context("failed to parse preamble facts")?;
            }
            Block::Section {
                heading,
                depth,
                raw_heading,
                blank_lines_before,
                fact_lines,
            } => {
                let facts = parse_facts(fact_lines)
                    .with_context(|| format!("failed to parse section '{heading}'"))?;
                let section = Section {
                    title: heading.clone(),
                    depth: *depth,
                    facts,
                    children: Vec::new(),
                    raw_heading: raw_heading.clone(),
                    blank_lines_before: *blank_lines_before,
                };
                sections.push(section);
            }
        }
    }

    // Build hierarchy from flat sections based on depth.
    let sections = build_hierarchy(sections);

    Ok(FactSheet {
        filename: filename.to_string(),
        preamble,
        sections,
    })
}

/// Intermediate block representation.
enum Block {
    Preamble(Vec<FactLine>),
    Section {
        heading: String,
        depth: usize,
        raw_heading: String,
        blank_lines_before: usize,
        fact_lines: Vec<FactLine>,
    },
}

/// A raw fact line (may span multiple lines for mappings).
#[derive(Debug, Clone)]
struct FactLine {
    raw: String,
    blank_lines_before: usize,
}

/// Split file lines into blocks: one preamble block and section blocks.
fn split_into_blocks<'a>(lines: &[&'a str]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current_fact_lines: Vec<FactLine> = Vec::new();
    let mut current_heading: Option<(String, usize, String, usize)> = None; // (title, depth, raw, blanks_before)
    let mut blank_count: usize = 0;
    let mut in_preamble = true;

    for line in lines {
        if line.trim().is_empty() {
            blank_count += 1;
            continue;
        }

        if let Some((depth, title)) = parse_heading(line) {
            // Save previous block
            if in_preamble {
                if !current_fact_lines.is_empty() {
                    blocks.push(Block::Preamble(std::mem::take(&mut current_fact_lines)));
                } else {
                    blocks.push(Block::Preamble(Vec::new()));
                }
                in_preamble = false;
            } else if let Some((h_title, h_depth, h_raw, h_blanks)) = current_heading.take() {
                blocks.push(Block::Section {
                    heading: h_title,
                    depth: h_depth,
                    raw_heading: h_raw,
                    blank_lines_before: h_blanks,
                    fact_lines: std::mem::take(&mut current_fact_lines),
                });
            }

            current_heading = Some((title, depth, line.to_string(), blank_count));
            blank_count = 0;
            continue;
        }

        // Fact line (- prefixed) or continuation of a mapping
        current_fact_lines.push(FactLine {
            raw: line.to_string(),
            blank_lines_before: blank_count,
        });
        blank_count = 0;
    }

    // Save last block
    if in_preamble {
        blocks.push(Block::Preamble(current_fact_lines));
    } else if let Some((h_title, h_depth, h_raw, h_blanks)) = current_heading.take() {
        blocks.push(Block::Section {
            heading: h_title,
            depth: h_depth,
            raw_heading: h_raw,
            blank_lines_before: h_blanks,
            fact_lines: current_fact_lines,
        });
    }

    blocks
}

/// Parse a heading line. Returns (depth, title) if it's a heading.
fn parse_heading(line: &str) -> Option<(usize, String)> {
    let trimmed = line.trim_start();
    if !trimmed.starts_with('#') {
        return None;
    }
    let depth = trimmed.chars().take_while(|&c| c == '#').count();
    let rest = trimmed[depth..].trim();
    if rest.is_empty() {
        return None;
    }
    Some((depth, rest.to_string()))
}

/// Parse fact lines into Fact structs.
/// Groups lines: a `- ` line starts a new fact, indented lines continue it.
fn parse_facts(lines: &[FactLine]) -> Result<Vec<Fact>> {
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    // Group lines into fact entries
    let mut entries: Vec<(Vec<String>, usize)> = Vec::new(); // (lines, blank_lines_before)

    for fl in lines {
        let line = &fl.raw;
        if line.starts_with("- ") || line == "-" {
            entries.push((vec![line.clone()], fl.blank_lines_before));
        } else if line.starts_with("  ") && !entries.is_empty() {
            // Continuation of previous mapping entry
            entries.last_mut().unwrap().0.push(line.clone());
        } else {
            bail!("unexpected line (not a fact or continuation): {line}");
        }
    }

    let mut facts = Vec::new();
    for (entry_lines, blank_lines_before) in entries {
        let fact = parse_single_fact(&entry_lines, blank_lines_before)?;
        facts.push(fact);
    }

    Ok(facts)
}

/// Parse a single fact from its raw lines.
fn parse_single_fact(lines: &[String], blank_lines_before: usize) -> Result<Fact> {
    let raw = lines.join("\n");

    if lines.len() == 1 {
        // Single line: could be plain string or single-line mapping
        let line = &lines[0];
        let content = line.strip_prefix("- ").unwrap_or(line);

        // Check if it's a single-line mapping (contains `: ` or ends with `:`)
        // But we need to be careful: "some fact: with colon" is still a plain string
        // A mapping would be like "- label: some text" where label is a known key
        // Or multi-line. For single line, check if it looks like a mapping.
        if is_single_line_mapping(content) {
            return parse_mapping_fact(lines, blank_lines_before);
        }

        // Plain string fact
        let (label, inline_tags) = extract_inline_tags(content);
        return Ok(Fact {
            explicit_id: None,
            label,
            command: None,
            tags: inline_tags,
            is_plain: true,
            raw,
            blank_lines_before,
        });
    }

    // Multi-line: mapping fact
    parse_mapping_fact(lines, blank_lines_before)
}

/// Check if a single-line content (after `- `) is a mapping entry.
fn is_single_line_mapping(content: &str) -> bool {
    // A mapping fact on a single line would be like:
    // - label: some text
    // - id: xyz
    // But "label:" as the only key on the line with known keys
    let known_keys = ["label:", "command:", "id:", "tags:"];
    for key in &known_keys {
        if content.starts_with(key) {
            return true;
        }
    }
    // Also check for {key: val, ...} YAML inline mapping syntax
    if content.starts_with('{') && content.ends_with('}') {
        return true;
    }
    false
}

/// Parse a mapping fact (multi-line or single-line mapping).
fn parse_mapping_fact(lines: &[String], blank_lines_before: usize) -> Result<Fact> {
    let raw = lines.join("\n");

    // Parse YAML key-value pairs from the mapping
    let mut label: Option<String> = None;
    let mut command: Option<String> = None;
    let mut explicit_id: Option<String> = None;
    let mut mapping_tags: Vec<String> = Vec::new();

    // Strip the leading `- ` from the first line, then parse key: value pairs
    let first_line = lines[0].strip_prefix("- ").unwrap_or(&lines[0]);

    // Collect all key-value lines
    let mut kv_lines: Vec<String> = vec![first_line.to_string()];
    for line in &lines[1..] {
        // Continuation lines have leading whitespace
        kv_lines.push(line.trim_start().to_string());
    }

    for kv_line in &kv_lines {
        if let Some(val) = kv_line.strip_prefix("label: ") {
            let (clean_label, inline_tags) = extract_inline_tags(val);
            label = Some(clean_label);
            mapping_tags.extend(inline_tags);
        } else if kv_line.starts_with("label:") && kv_line.trim() == "label:" {
            bail!("fact has empty label");
        } else if let Some(val) = kv_line.strip_prefix("command: ") {
            command = Some(val.to_string());
        } else if let Some(val) = kv_line.strip_prefix("id: ") {
            explicit_id = Some(val.to_string());
        } else if let Some(val) = kv_line.strip_prefix("tags: ") {
            // Parse YAML inline list: [tag1, tag2]
            let val = val.trim();
            if val.starts_with('[') && val.ends_with(']') {
                let inner = &val[1..val.len() - 1];
                for tag in inner.split(',') {
                    let tag = tag.trim();
                    if !tag.is_empty() {
                        mapping_tags.push(tag.to_string());
                    }
                }
            }
        } else if !kv_line.is_empty() {
            // Check for unknown keys
            if let Some(colon_pos) = kv_line.find(": ") {
                let key = &kv_line[..colon_pos];
                if !key.contains(' ') {
                    // Looks like a key: value pair with unknown key
                    let known = ["label", "command", "id", "tags"];
                    if !known.contains(&key) {
                        bail!("unknown key '{key}' in fact mapping");
                    }
                }
            }
        }
    }

    let label = label.context("mapping fact missing required 'label' key")?;

    // Deduplicate tags while preserving original order
    let mut seen = std::collections::HashSet::new();
    mapping_tags.retain(|t| seen.insert(t.clone()));

    Ok(Fact {
        explicit_id,
        label,
        command,
        tags: mapping_tags,
        is_plain: false,
        raw,
        blank_lines_before,
    })
}

/// Extract inline @tags from a string, returning (cleaned_label, tags).
pub fn extract_inline_tags(s: &str) -> (String, Vec<String>) {
    let mut tags = Vec::new();
    let mut parts = Vec::new();

    for word in s.split_whitespace() {
        if let Some(tag) = word.strip_prefix('@') {
            if !tag.is_empty() {
                tags.push(tag.to_string());
            }
        } else {
            parts.push(word);
        }
    }

    let label = parts.join(" ");
    (label, tags)
}

/// Build a tree hierarchy from flat sections based on depth.
fn build_hierarchy(flat: Vec<Section>) -> Vec<Section> {
    if flat.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<Section> = Vec::new();
    let mut stack: Vec<Section> = Vec::new();

    for section in flat {
        // Pop sections from the stack that are at the same or deeper level
        while let Some(top) = stack.last() {
            if top.depth >= section.depth {
                let popped = stack.pop().unwrap();
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(popped);
                } else {
                    result.push(popped);
                }
            } else {
                break;
            }
        }
        stack.push(section);
    }

    // Drain remaining stack
    while let Some(popped) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(popped);
        } else {
            result.push(popped);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_heading() {
        assert_eq!(parse_heading("# title"), Some((1, "title".to_string())));
        assert_eq!(
            parse_heading("## sub title"),
            Some((2, "sub title".to_string()))
        );
        assert_eq!(parse_heading("not a heading"), None);
        assert_eq!(parse_heading("- a fact"), None);
    }

    #[test]
    fn test_extract_inline_tags() {
        let (label, tags) = extract_inline_tags("some fact @mvp @core");
        assert_eq!(label, "some fact");
        assert_eq!(tags, vec!["mvp", "core"]);
    }

    #[test]
    fn test_extract_no_tags() {
        let (label, tags) = extract_inline_tags("a plain fact");
        assert_eq!(label, "a plain fact");
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_plain_fact() {
        let content = "- a simple fact\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.preamble.len(), 1);
        assert_eq!(sheet.preamble[0].label, "a simple fact");
        assert!(sheet.preamble[0].is_plain);
    }

    #[test]
    fn test_parse_plain_fact_with_tags() {
        let content = "- a tagged fact @mvp @core\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.preamble[0].label, "a tagged fact");
        assert_eq!(sheet.preamble[0].tags, vec!["mvp", "core"]);
    }

    #[test]
    fn test_parse_mapping_fact() {
        let content = "- label: project is a Cargo project\n  command: test -f Cargo.toml\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.preamble[0].label, "project is a Cargo project");
        assert_eq!(
            sheet.preamble[0].command.as_deref(),
            Some("test -f Cargo.toml")
        );
        assert!(!sheet.preamble[0].is_plain);
    }

    #[test]
    fn test_parse_sections() {
        let content = "# section one\n\n- fact one\n\n## subsection\n\n- fact two\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.sections.len(), 1);
        assert_eq!(sheet.sections[0].title, "section one");
        assert_eq!(sheet.sections[0].facts.len(), 1);
        assert_eq!(sheet.sections[0].children.len(), 1);
        assert_eq!(sheet.sections[0].children[0].title, "subsection");
        assert_eq!(sheet.sections[0].children[0].facts.len(), 1);
    }

    #[test]
    fn test_parse_empty_file() {
        let sheet = parse("", ".facts").unwrap();
        assert!(sheet.preamble.is_empty());
        assert!(sheet.sections.is_empty());
    }

    #[test]
    fn test_parse_headings_only() {
        let content = "# section one\n\n## subsection\n\n# section two\n";
        let sheet = parse(content, ".facts").unwrap();
        assert!(sheet.preamble.is_empty());
        assert_eq!(sheet.sections.len(), 2);
        assert_eq!(sheet.sections[0].title, "section one");
        assert_eq!(sheet.sections[0].facts.len(), 0);
        assert_eq!(sheet.sections[0].children.len(), 1);
        assert_eq!(sheet.sections[1].title, "section two");
        assert_eq!(sheet.sections[1].facts.len(), 0);
    }

    #[test]
    fn test_parse_deeply_nested_sections() {
        let content = "# l1\n\n## l2\n\n### l3\n\n#### l4\n\n- deep fact\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.sections.len(), 1);
        let l1 = &sheet.sections[0];
        assert_eq!(l1.title, "l1");
        assert_eq!(l1.children.len(), 1);
        let l2 = &l1.children[0];
        assert_eq!(l2.title, "l2");
        assert_eq!(l2.children.len(), 1);
        let l3 = &l2.children[0];
        assert_eq!(l3.title, "l3");
        assert_eq!(l3.children.len(), 1);
        let l4 = &l3.children[0];
        assert_eq!(l4.title, "l4");
        assert_eq!(l4.facts.len(), 1);
        assert_eq!(l4.facts[0].label, "deep fact");
    }

    #[test]
    fn test_parse_colon_in_plain_label() {
        // A plain fact with "note:" at the start — should be plain, not a mapping,
        // because "note" is not a known mapping key.
        let content = "- note: this has a colon\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.preamble.len(), 1);
        assert_eq!(sheet.preamble[0].label, "note: this has a colon");
        assert!(sheet.preamble[0].is_plain);
    }

    #[test]
    fn test_parse_command_with_pipe() {
        let content = "- label: pipe cmd\n  command: echo hello | grep hello\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(
            sheet.preamble[0].command.as_deref(),
            Some("echo hello | grep hello")
        );
    }

    #[test]
    fn test_parse_full_file() {
        let content = r#"# facts

- a CLI for fact-driven development with coding agents

## format

### file
- project root is the directory containing the nearest parent .git
- fact sheets are *.facts files in the project root
"#;
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.sections.len(), 1);
        assert_eq!(sheet.sections[0].title, "facts");
        assert_eq!(sheet.sections[0].facts.len(), 1);
        assert_eq!(sheet.sections[0].children.len(), 1);
        let format = &sheet.sections[0].children[0];
        assert_eq!(format.title, "format");
        assert_eq!(format.children.len(), 1);
        assert_eq!(format.children[0].title, "file");
        assert_eq!(format.children[0].facts.len(), 2);
    }

    #[test]
    fn test_parse_strips_utf8_bom() {
        let content = "\u{FEFF}- a fact with BOM\n";
        let sheet = parse(content, ".facts").unwrap();
        assert_eq!(sheet.preamble.len(), 1);
        assert_eq!(sheet.preamble[0].label, "a fact with BOM");
        assert!(sheet.preamble[0].is_plain);
    }
}
