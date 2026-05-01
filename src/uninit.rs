/// The `uninit` subcommand — remove facts initialization from a project.
///
/// Removes the default `.facts` file, agent skills from `.agents/skills/`,
/// and Claude symlinks from `.claude/skills/`. Idempotent: safe to run on
/// projects that were never initialized.
use anyhow::{Result, bail};
use std::path::Path;

use crate::project;

const SKILL_NAMES: &[&str] = &["facts", "facts-discover", "facts-implement"];

pub fn run(force: bool) -> Result<()> {
    let root = project::find_project_root()?;
    run_in(&root, force)
}

fn run_in(root: &Path, force: bool) -> Result<()> {
    let facts_path = root.join(".facts");
    if facts_path.exists() {
        let content = std::fs::read_to_string(&facts_path)?;
        if !content.trim().is_empty() && !force {
            bail!(".facts has content; use --force to delete");
        }
        std::fs::remove_file(&facts_path)?;
        println!("  remove  .facts");
    } else {
        println!("  skip  .facts (not found)");
    }

    for name in SKILL_NAMES {
        remove_skill(root, name)?;
        remove_claude_link(root, name)?;
    }

    crate::init::remove_agent_docs(root)?;

    remove_dir_if_empty(&root.join(".agents").join("skills"));
    remove_dir_if_empty(&root.join(".agents"));
    remove_dir_if_empty(&root.join(".claude").join("skills"));
    remove_dir_if_empty(&root.join(".claude"));

    Ok(())
}

fn remove_skill(root: &Path, name: &str) -> Result<()> {
    let skill_dir = root.join(".agents").join("skills").join(name);
    let skill_path = skill_dir.join("SKILL.md");

    if skill_path.exists() {
        std::fs::remove_file(&skill_path)?;
        remove_dir_if_empty(&skill_dir);
        println!("  remove  .agents/skills/{name}/SKILL.md");
    } else {
        println!("  skip  .agents/skills/{name}/SKILL.md (not found)");
    }

    Ok(())
}

fn remove_claude_link(root: &Path, name: &str) -> Result<()> {
    let link_path = root.join(".claude").join("skills").join(name);

    if link_path.is_symlink() {
        std::fs::remove_file(&link_path)?;
        println!("  remove  .claude/skills/{name} (symlink)");
    }

    Ok(())
}

fn remove_dir_if_empty(path: &Path) {
    if let Ok(mut entries) = std::fs::read_dir(path)
        && entries.next().is_none()
    {
        let _ = std::fs::remove_dir(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init;

    fn setup_initialized_project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

        init::run_test_init(dir.path()).unwrap();
        dir
    }

    #[test]
    fn test_uninit_removes_everything() {
        let dir = setup_initialized_project();

        assert!(dir.path().join(".facts").exists());
        assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());

        run_in(dir.path(), true).unwrap();

        assert!(!dir.path().join(".facts").exists());
        assert!(!dir.path().join(".agents/skills/facts/SKILL.md").exists());
        assert!(
            !dir.path()
                .join(".agents/skills/facts-discover/SKILL.md")
                .exists()
        );
        assert!(
            !dir.path()
                .join(".agents/skills/facts-implement/SKILL.md")
                .exists()
        );
    }

    #[test]
    fn test_uninit_cleans_empty_dirs() {
        let dir = setup_initialized_project();

        run_in(dir.path(), true).unwrap();

        assert!(!dir.path().join(".agents/skills").exists());
        assert!(!dir.path().join(".agents").exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_uninit_removes_claude_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        // Manually set up .agents/skills and .claude/skills symlinks
        // (don't use run_test_init which may or may not create symlinks
        // depending on whether Claude is available on the system).
        let agents_dir = dir.path().join(".agents").join("skills").join("facts");
        std::fs::create_dir_all(&agents_dir).unwrap();
        std::fs::write(agents_dir.join("SKILL.md"), "# test").unwrap();

        let link_dir = dir.path().join(".claude").join("skills");
        std::fs::create_dir_all(&link_dir).unwrap();
        let target = Path::new("..")
            .join("..")
            .join(".agents")
            .join("skills")
            .join("facts");
        std::os::unix::fs::symlink(&target, link_dir.join("facts")).unwrap();
        assert!(link_dir.join("facts").is_symlink());

        run_in(dir.path(), true).unwrap();

        assert!(!link_dir.join("facts").exists());
    }

    #[test]
    fn test_uninit_preserves_other_skills() {
        let dir = setup_initialized_project();

        let other = dir.path().join(".agents/skills/custom/SKILL.md");
        std::fs::create_dir_all(other.parent().unwrap()).unwrap();
        std::fs::write(&other, "# custom").unwrap();

        run_in(dir.path(), true).unwrap();

        assert!(!dir.path().join(".agents/skills/facts/SKILL.md").exists());
        assert!(other.exists());
        assert!(dir.path().join(".agents/skills").exists());
    }

    #[test]
    fn test_uninit_preserves_named_facts_files() {
        let dir = setup_initialized_project();
        std::fs::write(dir.path().join("cli.facts"), "- cli fact\n").unwrap();

        run_in(dir.path(), true).unwrap();

        assert!(!dir.path().join(".facts").exists());
        assert!(dir.path().join("cli.facts").exists());
    }

    #[test]
    fn test_uninit_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        assert!(run_in(dir.path(), false).is_ok());
        assert!(run_in(dir.path(), false).is_ok());
    }

    #[test]
    fn test_init_uninit_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

        init::run_test_init(dir.path()).unwrap();
        assert!(dir.path().join(".facts").exists());
        assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());

        run_in(dir.path(), true).unwrap();
        assert!(!dir.path().join(".facts").exists());
        assert!(!dir.path().join(".agents").exists());
    }
}
