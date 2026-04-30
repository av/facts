/// Core data model for fact sheets.

/// A parsed .facts file.
#[derive(Debug, Clone)]
pub struct FactSheet {
    /// The file name (e.g. ".facts", "cli.facts").
    pub filename: String,
    /// Top-level facts (before any heading).
    pub preamble: Vec<Fact>,
    /// Sections defined by headings.
    pub sections: Vec<Section>,
}

/// A section defined by a heading.
#[derive(Debug, Clone)]
pub struct Section {
    /// The heading text (without # prefix).
    pub title: String,
    /// The heading depth (number of # characters).
    pub depth: usize,
    /// Facts in this section (not including subsections).
    pub facts: Vec<Fact>,
    /// Child sections.
    pub children: Vec<Section>,
    /// Original heading line for deterministic write-back.
    pub raw_heading: String,
    /// Blank lines before this section's heading.
    pub blank_lines_before: usize,
}

/// A single fact.
#[derive(Debug, Clone)]
pub struct Fact {
    /// Explicit ID (from `id` key in mapping), if any.
    pub explicit_id: Option<String>,
    /// The label text (tags stripped).
    pub label: String,
    /// Optional validation command.
    pub command: Option<String>,
    /// Tags (from both inline and mapping sources).
    pub tags: Vec<String>,
    /// Whether this fact was originally a plain string (vs mapping).
    pub is_plain: bool,
    /// The original raw line(s) for deterministic write-back.
    pub raw: String,
    /// Blank lines before this fact.
    pub blank_lines_before: usize,
}

impl FactSheet {
    /// Iterate over all facts in file order, yielding (section_path, &Fact).
    pub fn all_facts(&self) -> Vec<(Vec<String>, &Fact)> {
        let mut result = Vec::new();
        for fact in &self.preamble {
            result.push((vec![], fact));
        }
        for section in &self.sections {
            Self::collect_facts(section, &[], &mut result);
        }
        result
    }

    fn collect_facts<'a>(
        section: &'a Section,
        parent_path: &[String],
        result: &mut Vec<(Vec<String>, &'a Fact)>,
    ) {
        let mut path = parent_path.to_vec();
        path.push(section.title.clone());
        for fact in &section.facts {
            result.push((path.clone(), fact));
        }
        for child in &section.children {
            Self::collect_facts(child, &path, result);
        }
    }

    /// Get the display name for the file (empty string for ".facts").
    pub fn display_name(&self) -> &str {
        if self.filename == ".facts" {
            ""
        } else {
            &self.filename
        }
    }
}
