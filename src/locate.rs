/// Shared fact-location utilities for mutation commands (remove, edit).
///
/// Provides a way to address individual facts by their position in a
/// FactSheet and navigate to them for reading or mutation.
use crate::model::{Fact, FactSheet, Section};

/// Location of a fact within a FactSheet.
#[derive(Debug, Clone)]
pub enum FactLocation {
    Preamble(usize),
    /// Section path as indices at each level, plus fact index within that section.
    Section(Vec<usize>, usize),
}

/// Recursively collect fact locations and labels from sections.
pub fn collect_section_locations(
    sheet_idx: usize,
    sections: &[Section],
    parent_indices: &[usize],
    all_labels: &mut Vec<(String, Option<String>)>,
    locations: &mut Vec<(usize, FactLocation)>,
) {
    for (sec_idx, section) in sections.iter().enumerate() {
        let mut path = parent_indices.to_vec();
        path.push(sec_idx);
        for (fact_idx, fact) in section.facts.iter().enumerate() {
            all_labels.push((fact.label.clone(), fact.explicit_id.clone()));
            locations.push((sheet_idx, FactLocation::Section(path.clone(), fact_idx)));
        }
        collect_section_locations(sheet_idx, &section.children, &path, all_labels, locations);
    }
}

/// Get a reference to a fact at a given location.
pub fn get_fact<'a>(sheet: &'a FactSheet, location: &FactLocation) -> &'a Fact {
    match location {
        FactLocation::Preamble(idx) => &sheet.preamble[*idx],
        FactLocation::Section(path, fact_idx) => {
            let section = navigate_to_section(&sheet.sections, path);
            &section.facts[*fact_idx]
        }
    }
}

/// Get a mutable reference to a fact at a given location.
pub fn get_fact_mut<'a>(sheet: &'a mut FactSheet, location: &FactLocation) -> &'a mut Fact {
    match location {
        FactLocation::Preamble(idx) => &mut sheet.preamble[*idx],
        FactLocation::Section(path, fact_idx) => {
            let section = navigate_to_section_mut(&mut sheet.sections, path);
            &mut section.facts[*fact_idx]
        }
    }
}

/// Navigate to a section by index path.
pub fn navigate_to_section<'a>(sections: &'a [Section], path: &[usize]) -> &'a Section {
    let mut current = &sections[path[0]];
    for &idx in &path[1..] {
        current = &current.children[idx];
    }
    current
}

/// Navigate to a mutable section by index path.
pub fn navigate_to_section_mut<'a>(sections: &'a mut [Section], path: &[usize]) -> &'a mut Section {
    let mut current = &mut sections[path[0]];
    for &idx in &path[1..] {
        current = &mut current.children[idx];
    }
    current
}
