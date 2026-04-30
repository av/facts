/// Project root detection and fact sheet discovery.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Find the project root by looking for the nearest parent directory containing `.git`.
pub fn find_project_root() -> Result<PathBuf> {
    let cwd =
        std::env::current_dir().context("failed to determine current directory")?;
    find_project_root_from(&cwd)
}

fn find_project_root_from(start: &Path) -> Result<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".git").exists() {
            return Ok(dir);
        }
        if !dir.pop() {
            anyhow::bail!(
                "no .git directory found in any parent of {}",
                start.display()
            );
        }
    }
}

/// Discover all .facts files in the project root directory (not recursive).
pub fn discover_fact_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root)
        .with_context(|| format!("failed to read directory {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".facts") {
                    files.push(path);
                }
            }
        }
    }
    // Sort for deterministic order, with .facts first
    files.sort_by(|a, b| {
        let a_name = a.file_name().unwrap().to_str().unwrap();
        let b_name = b.file_name().unwrap().to_str().unwrap();
        let a_is_default = a_name == ".facts";
        let b_is_default = b_name == ".facts";
        match (a_is_default, b_is_default) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a_name.cmp(b_name),
        }
    });
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_project_root() {
        // This test assumes we're running inside a git repo
        let root = find_project_root();
        assert!(root.is_ok());
        let root = root.unwrap();
        assert!(root.join(".git").exists());
    }
}
