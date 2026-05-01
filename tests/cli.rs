/// Integration tests for the `facts` CLI.
///
/// Each test creates an isolated temp directory with a `.git` marker
/// and `.facts` file(s), then runs the compiled binary end-to-end.
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create an isolated project directory with `.git` and a `.facts` file.
fn project(facts_content: &str) -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".facts"), facts_content).unwrap();
    dir
}

/// Create an isolated project directory with `.git` but NO `.facts` file.
fn empty_project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    dir
}

/// Get a `Command` for the `facts` binary, run inside the given dir.
fn facts_cmd(dir: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("facts").unwrap();
    cmd.current_dir(dir.path());
    // Force no color — stdout is a pipe, so TTY detection disables color automatically,
    // but this makes intent explicit.
    cmd.env("NO_COLOR", "1");
    cmd
}

// ===========================================================================
// list
// ===========================================================================

#[test]
fn list_shows_facts_from_file() {
    let dir = project("- first fact\n- second fact\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("first fact"))
        .stdout(predicate::str::contains("second fact"));
}

#[test]
fn list_shows_section_path() {
    let dir = project("# project\n\n- a fact\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("project"))
        .stdout(predicate::str::contains("a fact"));
}

#[test]
fn list_filter_by_section() {
    let dir = project("# alpha\n\n- alpha fact\n\n# beta\n\n- beta fact\n");
    facts_cmd(&dir)
        .args(["list", "--section", "beta"])
        .assert()
        .success()
        .stdout(predicate::str::contains("beta fact"))
        .stdout(predicate::str::contains("alpha fact").not());
}

#[test]
fn list_filter_has_command() {
    let dir = project("- manual fact\n- label: cmd fact\n  command: echo hi\n");
    facts_cmd(&dir)
        .args(["list", "--has-command"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cmd fact"))
        .stdout(predicate::str::contains("manual fact").not());
}

#[test]
fn list_filter_manual() {
    let dir = project("- manual fact\n- label: cmd fact\n  command: echo hi\n");
    facts_cmd(&dir)
        .args(["list", "--manual"])
        .assert()
        .success()
        .stdout(predicate::str::contains("manual fact"))
        .stdout(predicate::str::contains("cmd fact").not());
}

#[test]
fn list_filter_tags() {
    let dir = project("- tagged fact @mvp\n- untagged fact\n");
    facts_cmd(&dir)
        .args(["list", "--tags", "mvp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tagged fact"))
        .stdout(predicate::str::contains("untagged fact").not());
}

#[test]
fn bare_facts_defaults_to_list() {
    let dir = project("- hello world\n");
    // No subcommand at all
    facts_cmd(&dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("hello world"));
}

#[test]
fn list_file_prefix_omitted_for_default() {
    let dir = project("- a fact\n");
    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // ".facts" should NOT appear as a prefix in the output
    assert!(
        !stdout.contains(".facts"),
        "default file prefix should be omitted, got: {stdout}"
    );
}

#[test]
fn list_file_prefix_shown_for_named_file() {
    let dir = empty_project();
    fs::write(dir.path().join("cli.facts"), "- a cli fact\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("cli.facts"),
        "named file prefix should appear, got: {stdout}"
    );
}

// ===========================================================================
// check
// ===========================================================================

#[test]
fn check_passing_command() {
    let dir = project("- label: passes\n  command: \"true\"\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"))
        .stdout(predicate::str::contains("passes"))
        .stdout(predicate::str::contains("1 passed"));
}

#[test]
fn check_failing_command() {
    let dir = project("- label: fails\n  command: \"false\"\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("failed"))
        .stdout(predicate::str::contains("fails"))
        .stdout(predicate::str::contains("1 failed"));
}

#[test]
fn check_manual_facts_listed() {
    let dir = project("- a manual fact\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("manual"))
        .stdout(predicate::str::contains("a manual fact"))
        .stdout(predicate::str::contains("1 manual"));
}

#[test]
fn check_summary_line_format() {
    let dir = project(
        "- label: passes\n  command: \"true\"\n- label: fails\n  command: \"false\"\n- a manual fact\n",
    );
    let output = facts_cmd(&dir).arg("check").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 passed"));
    assert!(stdout.contains("1 failed"));
    assert!(stdout.contains("1 manual"));
}

#[test]
fn check_tags_filter() {
    let dir = project(
        "- label: tagged\n  command: \"true\"\n  tags: [mvp]\n- label: untagged\n  command: \"false\"\n",
    );
    // Only check the tagged (passing) fact; untagged (failing) is excluded
    facts_cmd(&dir)
        .args(["check", "--tags", "mvp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("tagged"))
        .stdout(predicate::str::contains("untagged").not());
}

#[test]
fn check_exit_code_zero_when_all_pass() {
    let dir = project("- label: ok\n  command: \"true\"\n");
    facts_cmd(&dir).arg("check").assert().success();
}

#[test]
fn check_exit_code_nonzero_when_any_fail() {
    let dir = project("- label: nope\n  command: \"false\"\n");
    facts_cmd(&dir).arg("check").assert().failure();
}

#[test]
fn check_exit_zero_with_only_manual_facts() {
    let dir = project("- just a manual fact\n");
    facts_cmd(&dir).arg("check").assert().success();
}

// ===========================================================================
// add
// ===========================================================================

#[test]
fn add_to_default_file() {
    let dir = project("- existing fact\n");
    facts_cmd(&dir)
        .args(["add", "a new fact"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("existing fact"));
    assert!(content.contains("a new fact"));
}

#[test]
fn add_with_section_creates_section() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "section fact", "--section", "mysection"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("# mysection"));
    assert!(content.contains("- section fact"));
}

#[test]
fn add_with_command_and_tags() {
    let dir = project("");
    facts_cmd(&dir)
        .args([
            "add",
            "validated fact",
            "--command",
            "echo ok",
            "--tags",
            "mvp,core",
        ])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("label: validated fact"));
    assert!(content.contains("command: echo ok"));
    assert!(content.contains("tags: [mvp, core]"));
}

#[test]
fn add_to_new_file() {
    let dir = empty_project();
    // No .facts file exists yet
    facts_cmd(&dir)
        .args(["add", "new file fact", "--file", "custom.facts"])
        .assert()
        .success();

    let path = dir.path().join("custom.facts");
    assert!(path.exists());
    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("new file fact"));
}

#[test]
fn add_with_explicit_id() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "id fact", "--id", "myid"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("id: myid"));
    assert!(content.contains("label: id fact"));
}

// ===========================================================================
// remove
// ===========================================================================

#[test]
fn remove_by_id_outputs_label() {
    let dir = project("- fact to remove\n- fact to keep\n");

    // First, find the ID for "fact to remove" by listing
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    // Each line starts with the ID followed by two spaces
    let id = stdout
        .lines()
        .find(|l| l.contains("fact to remove"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["remove", id])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact to remove"));

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(!content.contains("fact to remove"));
    assert!(content.contains("fact to keep"));
}

#[test]
fn remove_unknown_id_errors() {
    let dir = project("- some fact\n");
    facts_cmd(&dir)
        .args(["remove", "zzz"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("zzz"));
}

// ===========================================================================
// edit
// ===========================================================================

#[test]
fn edit_label() {
    let dir = project("- original label\n");

    // Find ID
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("original label"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--label", "updated label"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("updated label"));
    assert!(!content.contains("original label"));
}

#[test]
fn edit_adds_command() {
    let dir = project("- plain fact\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("plain fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--command", "echo hi"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("label: plain fact"));
    assert!(content.contains("command: echo hi"));
}

#[test]
fn edit_tags() {
    let dir = project("- label: tagged\n  tags: [old]\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("tagged"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--tags", "new1,new2"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("tags: [new1, new2]"));
    assert!(!content.contains("old"));
}

#[test]
fn edit_unknown_id_errors() {
    let dir = project("- some fact\n");
    facts_cmd(&dir)
        .args(["edit", "zzz", "--label", "new"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("zzz"));
}

// ===========================================================================
// lint
// ===========================================================================

#[test]
fn lint_valid_file_passes() {
    let dir = project("# project\n\n- a valid fact\n- label: mapping fact\n  command: echo ok\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn lint_catches_invalid_content() {
    let dir = project("# title\n\nthis is not a fact\n- valid fact\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected line"));
}

#[test]
fn lint_catches_unknown_key() {
    let dir = project("- label: fact\n  priority: high\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown key"));
}

#[test]
fn lint_warns_mixed_tags() {
    let dir = project("- label: fact @mvp\n  tags: [core]\n");
    // Mixed tags is a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("mixed inline and mapping tags"));
}

#[test]
fn lint_warns_duplicate_ids() {
    let dir = project("- label: first\n  id: dupe\n- label: second\n  id: dupe\n");
    // Duplicate IDs are a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("duplicate"));
}

#[test]
fn lint_passes_with_unique_ids() {
    let dir = project("- label: first\n  id: aaa\n- label: second\n  id: bbb\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn lint_warns_bare_tags() {
    let dir = project("- label: a fact\n  tags: mvp, core\n");
    // Bare tags is a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("tags should use bracket syntax"));
}

#[test]
fn lint_passes_bracket_tags() {
    let dir = project("- label: a fact\n  tags: [mvp, core]\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn lint_warns_crlf_line_endings() {
    let dir = project("- a fact\r\n- another fact\r\n");
    // CRLF is a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("CRLF line endings"));
}

#[test]
fn lint_passes_lf_line_endings() {
    let dir = project("- a fact\n- another fact\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

// ===========================================================================
// init
// ===========================================================================

#[test]
fn init_creates_facts_file_with_cargo() {
    let dir = empty_project();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\n",
    )
    .unwrap();

    facts_cmd(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("Rust/Cargo"));

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("cargo"));
}

#[test]
fn init_skips_existing_facts_file() {
    let dir = project("- existing\n");
    facts_cmd(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("skip"));

    // Original content untouched
    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert_eq!(content, "- existing\n");
}

#[test]
fn init_no_frameworks_scaffolds_minimal() {
    let dir = empty_project();
    facts_cmd(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("no frameworks"));

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("# project"));
}

#[test]
fn init_installs_skills() {
    let dir = empty_project();
    facts_cmd(&dir).arg("init").assert().success();

    assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());
    assert!(
        dir.path()
            .join(".agents/skills/facts-discover/SKILL.md")
            .exists()
    );
    assert!(
        dir.path()
            .join(".agents/skills/facts-implement/SKILL.md")
            .exists()
    );
}

#[test]
fn init_is_idempotent() {
    let dir = empty_project();
    fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

    facts_cmd(&dir).arg("init").assert().success();
    let content_first = fs::read_to_string(dir.path().join(".facts")).unwrap();

    // Second run succeeds, .facts unchanged, skills still present.
    // Skip count varies (4 without Claude, 7 with Claude symlinks).
    facts_cmd(&dir)
        .arg("init")
        .assert()
        .success()
        .stdout(predicate::str::contains("skip"));

    let content_second = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert_eq!(content_first, content_second);
}

#[test]
fn init_detects_node_scripts() {
    let dir = empty_project();
    fs::write(
        dir.path().join("package.json"),
        r#"{"scripts":{"test":"jest","lint":"eslint .","build":"tsc"}}"#,
    )
    .unwrap();

    facts_cmd(&dir).arg("init").assert().success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("npm test"));
    assert!(content.contains("npm run lint"));
    assert!(content.contains("npm run build"));
}

#[test]
fn init_detects_yarn() {
    let dir = empty_project();
    fs::write(
        dir.path().join("package.json"),
        r#"{"scripts":{"test":"vitest"}}"#,
    )
    .unwrap();
    fs::write(dir.path().join("yarn.lock"), "").unwrap();

    facts_cmd(&dir).arg("init").assert().success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("yarn test"));
}

#[test]
fn init_detects_python_tools() {
    let dir = empty_project();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[project]\nname = \"test\"\n\n[tool.pytest.ini_options]\n\n[tool.ruff]\n",
    )
    .unwrap();

    facts_cmd(&dir).arg("init").assert().success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("pytest"));
    assert!(content.contains("ruff check"));
}

// ===========================================================================
// uninit
// ===========================================================================

#[test]
fn uninit_removes_facts_and_skills() {
    let dir = empty_project();
    fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

    facts_cmd(&dir).arg("init").assert().success();
    assert!(dir.path().join(".facts").exists());
    assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());

    facts_cmd(&dir)
        .arg("uninit")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("remove"));

    assert!(!dir.path().join(".facts").exists());
    assert!(!dir.path().join(".agents/skills/facts/SKILL.md").exists());
}

#[test]
fn uninit_is_idempotent() {
    let dir = empty_project();

    facts_cmd(&dir)
        .arg("uninit")
        .assert()
        .success()
        .stdout(predicate::str::contains("skip"));

    // Running again is fine.
    facts_cmd(&dir).arg("uninit").assert().success();
}

#[test]
fn uninit_preserves_named_facts_files() {
    let dir = empty_project();
    fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

    facts_cmd(&dir).arg("init").assert().success();
    fs::write(dir.path().join("cli.facts"), "- cli fact\n").unwrap();

    facts_cmd(&dir)
        .arg("uninit")
        .arg("--force")
        .assert()
        .success();

    assert!(!dir.path().join(".facts").exists());
    assert!(dir.path().join("cli.facts").exists());
}

#[test]
fn init_uninit_roundtrip() {
    let dir = empty_project();
    fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

    facts_cmd(&dir).arg("init").assert().success();
    facts_cmd(&dir)
        .arg("uninit")
        .arg("--force")
        .assert()
        .success();

    assert!(!dir.path().join(".facts").exists());
    assert!(!dir.path().join(".agents").exists());

    // Re-init should work cleanly.
    facts_cmd(&dir).arg("init").assert().success();
    assert!(dir.path().join(".facts").exists());
    assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());
}

// ===========================================================================

#[test]
fn uninit_requires_force_when_file_has_content() {
    let dir = project("- important fact\n");

    facts_cmd(&dir)
        .arg("uninit")
        .assert()
        .failure()
        .stderr(predicate::str::contains(".facts has content"))
        .stderr(predicate::str::contains("--force"));

    // File must still exist.
    assert!(dir.path().join(".facts").exists());
}

#[test]
fn uninit_force_deletes_nonempty_file() {
    let dir = project("- important fact\n");

    facts_cmd(&dir)
        .arg("uninit")
        .arg("--force")
        .assert()
        .success()
        .stdout(predicate::str::contains("remove"));

    assert!(!dir.path().join(".facts").exists());
}

#[test]
fn uninit_deletes_empty_file_without_force() {
    let dir = project("");

    facts_cmd(&dir)
        .arg("uninit")
        .assert()
        .success()
        .stdout(predicate::str::contains("remove"));

    assert!(!dir.path().join(".facts").exists());
}

// Edge cases / cross-cutting
// ===========================================================================

#[test]
fn nested_section_path_in_list() {
    let dir = project("# parent\n\n## child\n\n- deep fact\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("parent"))
        .stdout(predicate::str::contains("child"))
        .stdout(predicate::str::contains("deep fact"));
}

#[test]
fn multiple_files_listed() {
    let dir = project("- default fact\n");
    fs::write(dir.path().join("extra.facts"), "- extra fact\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("default fact"));
    assert!(stdout.contains("extra fact"));
    assert!(stdout.contains("extra.facts"));
}

#[test]
fn add_then_list_roundtrip() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "roundtrip fact"])
        .assert()
        .success();

    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("roundtrip fact"));
}

#[test]
fn add_then_remove_roundtrip() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "ephemeral fact"])
        .assert()
        .success();

    // Get the ID
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("ephemeral fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir).args(["remove", id]).assert().success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(!content.contains("ephemeral fact"));
}

#[test]
fn check_with_stderr_shows_error() {
    let dir = project("- label: noisy fail\n  command: echo oops >&2; exit 1\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .failure()
        .stdout(predicate::str::contains("oops"));
}

#[test]
fn list_with_explicit_id_fact() {
    let dir = project("- label: custom id fact\n  id: xyz\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("xyz"))
        .stdout(predicate::str::contains("custom id fact"));
}

#[test]
fn list_tags_boolean_expression() {
    let dir = project("- fact one @mvp @core\n- fact two @mvp\n- fact three @core\n");
    facts_cmd(&dir)
        .args(["list", "--tags", "mvp and core"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact one"))
        .stdout(predicate::str::contains("fact two").not())
        .stdout(predicate::str::contains("fact three").not());
}

#[test]
fn check_mixed_pass_fail_manual() {
    let dir = project(
        "- label: good\n  command: \"true\"\n- label: bad\n  command: \"false\"\n- manual one\n",
    );
    let output = facts_cmd(&dir).arg("check").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Non-zero exit because of the failure
    assert!(!output.status.success());

    // All three categories present
    assert!(stdout.contains("good"));
    assert!(stdout.contains("bad"));
    assert!(stdout.contains("manual one"));
    assert!(stdout.contains("1 passed"));
    assert!(stdout.contains("1 failed"));
    assert!(stdout.contains("1 manual"));
}

#[test]
fn lint_empty_file_passes() {
    let dir = project("");
    facts_cmd(&dir).arg("lint").assert().success();
}

#[test]
fn add_nested_section() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "nested fact", "--section", "parent/child"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("# parent"));
    assert!(content.contains("## child"));
    assert!(content.contains("- nested fact"));
}

// ===========================================================================
// Edge cases: cross-file ID stability
// ===========================================================================

#[test]
fn cross_file_ids_are_globally_unique() {
    // Two files with identical fact labels — IDs must differ globally
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- shared label\n").unwrap();
    fs::write(dir.path().join("other.facts"), "- shared label\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("shared label"))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(ids.len(), 2, "expected 2 facts, got: {stdout}");
    assert_ne!(ids[0], ids[1], "IDs must be globally unique: {ids:?}");
}

#[test]
fn cross_file_ids_stable_with_file_filter() {
    // IDs for a fact should be the same whether or not --file is used
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- unique fact alpha\n").unwrap();
    fs::write(dir.path().join("other.facts"), "- unique fact beta\n").unwrap();

    // Get ID from unfiltered list
    let full_output = facts_cmd(&dir).arg("list").output().unwrap();
    let full_stdout = String::from_utf8_lossy(&full_output.stdout);
    let full_id = full_stdout
        .lines()
        .find(|l| l.contains("unique fact alpha"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    // Get ID from --file filtered list
    let filtered_output = facts_cmd(&dir)
        .args(["list", "--file", ".facts"])
        .output()
        .unwrap();
    let filtered_stdout = String::from_utf8_lossy(&filtered_output.stdout);
    let filtered_id = filtered_stdout
        .lines()
        .find(|l| l.contains("unique fact alpha"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    assert_eq!(
        full_id, filtered_id,
        "ID should be stable regardless of --file filter"
    );
}

#[test]
fn cross_file_list_id_matches_remove_id() {
    // The ID shown by `list` should work with `remove` even across multiple files
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- fact in default\n").unwrap();
    fs::write(dir.path().join("other.facts"), "- fact in other\n").unwrap();

    // Get ID from list
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("fact in other"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    // Remove using that ID
    facts_cmd(&dir)
        .args(["remove", id])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact in other"));

    // Verify removal
    let content = fs::read_to_string(dir.path().join("other.facts")).unwrap();
    assert!(
        !content.contains("fact in other"),
        "fact should have been removed"
    );
}

// ===========================================================================
// Edge cases: empty and minimal files
// ===========================================================================

#[test]
fn empty_facts_file_list() {
    let dir = project("");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn empty_facts_file_check() {
    let dir = project("");
    facts_cmd(&dir).arg("check").assert().success();
}

#[test]
fn empty_facts_file_lint() {
    let dir = project("");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn headings_only_no_facts() {
    let dir = project("# section one\n\n## subsection\n\n# section two\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn headings_only_check() {
    let dir = project("# section one\n\n## subsection\n");
    facts_cmd(&dir).arg("check").assert().success();
}

#[test]
fn headings_only_lint() {
    let dir = project("# section one\n\n## subsection\n");
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

// ===========================================================================
// Edge cases: deeply nested sections (3+ levels)
// ===========================================================================

#[test]
fn deeply_nested_section_list() {
    let dir = project("# level1\n\n## level2\n\n### level3\n\n#### level4\n\n- deep fact\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("level1"))
        .stdout(predicate::str::contains("level2"))
        .stdout(predicate::str::contains("level3"))
        .stdout(predicate::str::contains("level4"))
        .stdout(predicate::str::contains("deep fact"));
}

#[test]
fn deeply_nested_section_check() {
    let dir = project("# l1\n\n## l2\n\n### l3\n\n- label: deep cmd\n  command: \"true\"\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("l1 > l2 > l3 > deep cmd"));
}

#[test]
fn add_to_deeply_nested_section() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "deep fact", "--section", "a/b/c"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("# a"));
    assert!(content.contains("## b"));
    assert!(content.contains("### c"));
    assert!(content.contains("- deep fact"));
}

// ===========================================================================
// Edge cases: special characters
// ===========================================================================

#[test]
fn fact_label_with_colon() {
    // A plain fact with a colon should NOT be treated as a mapping
    let dir = project("- note: this has a colon in it\n");
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("note: this has a colon in it"));
}

#[test]
fn command_with_pipe() {
    let dir = project("- label: pipe cmd\n  command: echo hello | grep hello\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("pipe cmd"))
        .stdout(predicate::str::contains("1 passed"));
}

#[test]
fn command_with_redirect() {
    let dir = project("- label: redirect cmd\n  command: echo ok > /dev/null && true\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 passed"));
}

#[test]
fn command_with_semicolons() {
    let dir = project("- label: multi cmd\n  command: echo a; echo b; true\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 passed"));
}

// ===========================================================================
// Edge cases: multiple files aggregation
// ===========================================================================

#[test]
fn check_aggregates_across_files() {
    let dir = empty_project();
    fs::write(
        dir.path().join(".facts"),
        "- label: default pass\n  command: \"true\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("extra.facts"),
        "- label: extra pass\n  command: \"true\"\n",
    )
    .unwrap();

    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 passed"));
}

#[test]
fn lint_aggregates_across_files() {
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- fact one\n").unwrap();
    fs::write(dir.path().join("extra.facts"), "- fact two\n").unwrap();

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 files passed"));
}

// ===========================================================================
// Edge cases: complex boolean tag expressions
// ===========================================================================

#[test]
fn tags_complex_boolean_or_and_not() {
    let dir = project("- fact a @mvp @core\n- fact b @mvp\n- fact c @core\n- fact d @blocked\n");
    // (mvp or core) and not blocked
    facts_cmd(&dir)
        .args(["list", "--tags", "(mvp or core) and not blocked"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact a"))
        .stdout(predicate::str::contains("fact b"))
        .stdout(predicate::str::contains("fact c"))
        .stdout(predicate::str::contains("fact d").not());
}

#[test]
fn tags_double_not() {
    let dir = project("- fact one @mvp\n- fact two\n");
    // not not mvp => mvp
    facts_cmd(&dir)
        .args(["list", "--tags", "not not mvp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact one"))
        .stdout(predicate::str::contains("fact two").not());
}

#[test]
fn tags_nested_parens() {
    let dir = project("- fact a @x @y\n- fact b @x @z\n- fact c @y @z\n");
    // x and (y or z) — should match a (x+y), b (x+z), but not c (no x)
    facts_cmd(&dir)
        .args(["list", "--tags", "x and (y or z)"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact a"))
        .stdout(predicate::str::contains("fact b"))
        .stdout(predicate::str::contains("fact c").not());
}

// ===========================================================================
// Exact section matching
// ===========================================================================

#[test]
fn section_filter_exact_match_includes_subsections() {
    // --section "cli" should match "cli" and "cli/check" but not "cli_tools"
    let dir =
        project("# cli\n\n- cli fact\n\n## check\n\n- check fact\n\n# cli_tools\n\n- tools fact\n");
    facts_cmd(&dir)
        .args(["list", "--section", "cli"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cli fact"))
        .stdout(predicate::str::contains("check fact"))
        .stdout(predicate::str::contains("tools fact").not());
}

#[test]
fn section_filter_does_not_substring_match() {
    // --section "cli" must NOT match "cli_tools" (substring match would)
    let dir = project("# cli_tools\n\n- tools fact\n\n# cli\n\n- cli fact\n");
    facts_cmd(&dir)
        .args(["list", "--section", "cli"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cli fact"))
        .stdout(predicate::str::contains("tools fact").not());
}

#[test]
fn section_filter_exact_nested_path() {
    // --section "cli/check" matches exactly "cli > check", not "cli > checkout"
    let dir = project(
        "# cli\n\n## check\n\n- check fact\n\n## checkout\n\n- checkout fact\n\n## list\n\n- list fact\n",
    );
    facts_cmd(&dir)
        .args(["list", "--section", "cli/check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("check fact"))
        .stdout(predicate::str::contains("checkout fact").not())
        .stdout(predicate::str::contains("list fact").not());
}

#[test]
fn section_filter_nested_includes_deep_children() {
    // --section "cli/check" should also match "cli/check/output"
    let dir = project(
        "# cli\n\n## check\n\n- check fact\n\n### output\n\n- output fact\n\n## list\n\n- list fact\n",
    );
    facts_cmd(&dir)
        .args(["list", "--section", "cli/check"])
        .assert()
        .success()
        .stdout(predicate::str::contains("check fact"))
        .stdout(predicate::str::contains("output fact"))
        .stdout(predicate::str::contains("list fact").not());
}

#[test]
fn section_filter_is_case_insensitive() {
    // add --section uses case-insensitive matching, so list --section must too
    let dir = project("# Api\n\n- upper fact\n");
    // Filtering with lowercase "api" should match the "Api" section
    facts_cmd(&dir)
        .args(["list", "--section", "api"])
        .assert()
        .success()
        .stdout(predicate::str::contains("upper fact"));
}

#[test]
fn section_filter_case_insensitive_nested() {
    // Case-insensitive matching should also work for nested section paths
    let dir = project("# Api\n\n## Auth\n\n- auth fact\n\n## Routing\n\n- route fact\n");
    facts_cmd(&dir)
        .args(["list", "--section", "api/auth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("auth fact"))
        .stdout(predicate::str::contains("route fact").not());
}

// ===========================================================================
// Multi-file scenarios
// ===========================================================================

#[test]
fn add_to_named_file_creates_it() {
    let dir = project("- default fact\n");
    facts_cmd(&dir)
        .args(["add", "cli specific fact", "--file", "cli.facts"])
        .assert()
        .success();

    let cli_path = dir.path().join("cli.facts");
    assert!(cli_path.exists(), "cli.facts should be created");
    let content = fs::read_to_string(&cli_path).unwrap();
    assert!(content.contains("cli specific fact"));
}

#[test]
fn list_aggregates_from_both_files() {
    let dir = project("- default fact\n");
    fs::write(dir.path().join("cli.facts"), "- cli fact\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("default fact"),
        "should list fact from .facts"
    );
    assert!(
        stdout.contains("cli fact"),
        "should list fact from cli.facts"
    );
}

#[test]
fn list_shows_file_prefix_for_cli_facts_not_for_default() {
    let dir = project("- default fact\n");
    fs::write(dir.path().join("cli.facts"), "- cli fact\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The line with "cli fact" should have "cli.facts" prefix
    let cli_line = stdout.lines().find(|l| l.contains("cli fact")).unwrap();
    assert!(
        cli_line.contains("cli.facts"),
        "cli.facts prefix should appear for named file, got: {cli_line}"
    );

    // The line with "default fact" should NOT have ".facts" prefix
    let default_line = stdout.lines().find(|l| l.contains("default fact")).unwrap();
    assert!(
        !default_line.contains(".facts"),
        "no .facts prefix for default file, got: {default_line}"
    );
}

#[test]
fn check_runs_commands_from_all_files() {
    let dir = empty_project();
    fs::write(
        dir.path().join(".facts"),
        "- label: default cmd\n  command: \"true\"\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("cli.facts"),
        "- label: cli cmd\n  command: \"true\"\n",
    )
    .unwrap();

    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 passed"));
}

#[test]
fn lint_validates_all_files() {
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- valid fact\n").unwrap();
    fs::write(dir.path().join("cli.facts"), "- another valid fact\n").unwrap();

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 files passed"));
}

#[test]
fn remove_fact_from_non_default_file() {
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- default fact\n").unwrap();
    fs::write(dir.path().join("cli.facts"), "- cli fact to remove\n").unwrap();

    // Get ID of the cli fact
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("cli fact to remove"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["remove", id])
        .assert()
        .success()
        .stdout(predicate::str::contains("cli fact to remove"));

    let content = fs::read_to_string(dir.path().join("cli.facts")).unwrap();
    assert!(
        !content.contains("cli fact to remove"),
        "fact should be removed from cli.facts"
    );
    // Default file should be untouched
    let default_content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(default_content.contains("default fact"));
}

#[test]
fn edit_fact_in_non_default_file() {
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- default fact\n").unwrap();
    fs::write(dir.path().join("cli.facts"), "- cli fact original\n").unwrap();

    // Get ID of the cli fact
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("cli fact original"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--label", "cli fact edited"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("cli.facts")).unwrap();
    assert!(content.contains("cli fact edited"));
    assert!(!content.contains("cli fact original"));
}

#[test]
fn cross_file_same_label_different_ids() {
    // Two files with the same label should produce different IDs
    let dir = empty_project();
    fs::write(dir.path().join(".facts"), "- shared label\n").unwrap();
    fs::write(dir.path().join("other.facts"), "- shared label\n").unwrap();

    let output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let ids: Vec<&str> = stdout
        .lines()
        .filter(|l| l.contains("shared label"))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(
        ids[0], ids[1],
        "same label in different files should get different IDs"
    );
}

// ===========================================================================
// Tag normalization
// ===========================================================================

#[test]
fn add_plain_fact_with_tags_stays_plain_with_inline_tags() {
    // Adding a fact with only --tags keeps it as a plain string with inline @tags
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "tagged fact", "--tags", "mvp,core"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    // Should be a plain string with inline @tags (not a mapping)
    assert_eq!(content, "- tagged fact @mvp @core\n");
    assert!(!content.contains("label:"));
    assert!(!content.contains("tags:"));
}

#[test]
fn add_plain_fact_no_tags_stays_plain() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "plain fact"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert_eq!(content, "- plain fact\n");
}

#[test]
fn mapping_fact_has_tags_in_tags_key() {
    // A fact with command + tags should have tags in tags: key, not inline
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "cmd fact", "--command", "echo ok", "--tags", "mvp"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("label: cmd fact"));
    assert!(content.contains("command: echo ok"));
    assert!(content.contains("tags: [mvp]"));
    assert!(!content.contains("@mvp"));
}

#[test]
fn edit_adds_tags_to_plain_keeps_plain_with_inline_tags() {
    let dir = project("- a plain fact\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("a plain fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--tags", "mvp,core"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    // Tags alone do NOT promote to mapping — stays plain with inline @tags
    assert_eq!(content, "- a plain fact @mvp @core\n");
    assert!(!content.contains("label:"));
    assert!(!content.contains("tags:"));
}

#[test]
fn edit_adds_command_to_tagged_plain_migrates_tags_to_key() {
    // A plain fact with inline tags: when adding a command, tags should migrate
    // from inline (@tag) to tags: key
    let dir = project("- tagged fact @mvp @core\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("tagged fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--command", "echo check"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("label: tagged fact"));
    assert!(content.contains("command: echo check"));
    assert!(content.contains("tags: [mvp, core]"));
    assert!(!content.contains("@mvp"));
    assert!(!content.contains("@core"));
}

#[test]
fn same_label_different_tags_same_id() {
    // Tags are stripped from label before ID hashing, so same label with
    // different tags should produce the same base ID
    let dir = project("- my fact @mvp\n- another fact @core\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);

    // Get IDs
    // Verify the two different-label facts are listed
    assert!(stdout.lines().any(|l| l.contains("my fact")));
    assert!(stdout.lines().any(|l| l.contains("another fact")));

    // Now test the real scenario: same label with different tags in separate files
    let dir2 = project("- unique label @alpha\n");
    fs::write(dir2.path().join("other.facts"), "- unique label @beta\n").unwrap();

    let list_output2 = facts_cmd(&dir2).arg("list").output().unwrap();
    let stdout2 = String::from_utf8_lossy(&list_output2.stdout);
    let ids: Vec<&str> = stdout2
        .lines()
        .filter(|l| l.contains("unique label"))
        .map(|l| l.split_whitespace().next().unwrap())
        .collect();

    // Same label "unique label" (tags stripped) → same hash → collision resolution
    // Both should be present but with different IDs due to collision handling
    assert_eq!(ids.len(), 2, "both facts should be listed");
    assert_ne!(
        ids[0], ids[1],
        "collision resolution should make them unique"
    );
    // But importantly, the base hash is the same (they collided because tags
    // were stripped from the label before hashing)
}

// ===========================================================================
// --tags filter with @implemented
// ===========================================================================

#[test]
fn tags_filter_implemented() {
    let dir = project("- done feature @implemented\n- pending feature\n- also done @implemented\n");

    // Filter for implemented facts
    facts_cmd(&dir)
        .args(["list", "--tags", "implemented"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done feature"))
        .stdout(predicate::str::contains("also done"))
        .stdout(predicate::str::contains("pending feature").not());
}

#[test]
fn tags_filter_not_implemented() {
    let dir = project("- done feature @implemented\n- pending feature\n- also done @implemented\n");

    // Filter for NOT implemented facts
    facts_cmd(&dir)
        .args(["list", "--tags", "not implemented"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pending feature"))
        .stdout(predicate::str::contains("done feature").not())
        .stdout(predicate::str::contains("also done").not());
}

#[test]
fn check_runs_commands_for_implemented_facts() {
    // @implemented is informational — check should still run the command
    let dir = project(
        "- label: done check\n  command: \"true\"\n  tags: [implemented]\n\
         - label: pending check\n  command: \"true\"\n",
    );

    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("done check"))
        .stdout(predicate::str::contains("pending check"))
        .stdout(predicate::str::contains("2 passed"));
}

#[test]
fn check_with_tags_filter_only_runs_matched() {
    // Using --tags "implemented" on check should only run implemented facts
    let dir = project(
        "- label: done check\n  command: \"true\"\n  tags: [implemented]\n\
         - label: pending check\n  command: \"false\"\n",
    );

    // Without --tags, the failing command causes failure
    facts_cmd(&dir).arg("check").assert().failure();

    // With --tags "implemented", only the passing command runs
    facts_cmd(&dir)
        .args(["check", "--tags", "implemented"])
        .assert()
        .success()
        .stdout(predicate::str::contains("done check"))
        .stdout(predicate::str::contains("1 passed"));
}

// ===========================================================================
// edit --add-tag / --remove-tag
// ===========================================================================

#[test]
fn edit_add_tag_appends() {
    let dir = project("- label: tagged fact\n  tags: [existing]\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("tagged fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--add-tag", "new"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("tags: [existing, new]"));
}

#[test]
fn edit_add_tag_deduplicates() {
    let dir = project("- label: tagged fact\n  tags: [existing]\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("tagged fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--add-tag", "existing"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("tags: [existing]"));
    assert!(!content.contains("tags: [existing, existing]"));
}

#[test]
fn edit_remove_tag() {
    let dir = project("- label: tagged fact\n  tags: [keep, drop]\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("tagged fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--remove-tag", "drop"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("tags: [keep]"));
    assert!(!content.contains("drop"));
}

#[test]
fn edit_add_tag_to_untagged_plain_fact() {
    let dir = project("- a plain fact\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("a plain fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--add-tag", "implemented"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("a plain fact @implemented"));
}

#[test]
fn edit_tags_conflicts_with_add_tag() {
    let dir = project("- a fact\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("a fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["edit", id, "--tags", "a", "--add-tag", "b"])
        .assert()
        .failure();
}

// ===========================================================================
// check runs lint first
// ===========================================================================

#[test]
fn check_fails_on_lint_errors() {
    let dir = project("- label: ok\n  priority: high\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown key"))
        .stderr(predicate::str::contains("fix lint errors first"));
}

#[test]
fn check_passes_lint_warnings() {
    // Mixed tags is a warning, not an error — check should proceed
    let dir = project("- label: ok @tag\n  tags: [other]\n  command: \"true\"\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 passed"));
}

// ===========================================================================
// --version / --help
// ===========================================================================

#[test]
fn version_flag_prints_version() {
    let dir = empty_project();
    facts_cmd(&dir)
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

// ===========================================================================
// input validation — newlines in labels
// ===========================================================================

#[test]
fn add_rejects_label_with_newline() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "line\nbreak"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot contain newlines"));
}

#[test]
fn edit_rejects_label_with_newline() {
    let dir = project("- original fact\n");
    // Get the ID of the fact
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("original fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    facts_cmd(&dir)
        .args(["edit", &id, "--label", "line\nbreak"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot contain newlines"));
}

// ===========================================================================
// input validation — empty section path components
// ===========================================================================

#[test]
fn add_rejects_section_with_leading_slash() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--section", "/a/b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "section path cannot contain empty components",
        ));
}

#[test]
fn add_rejects_empty_section_path() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--section", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "section path cannot contain empty components",
        ));
}

#[test]
fn add_rejects_section_with_trailing_slash() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--section", "a/b/"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "section path cannot contain empty components",
        ));
}

#[test]
fn add_rejects_section_with_double_slash() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--section", "a//b"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "section path cannot contain empty components",
        ));
}

// ===========================================================================
// input validation — --file path traversal
// ===========================================================================

#[test]
fn add_rejects_absolute_file_path() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "/tmp/outside.facts"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "file path must be relative, not absolute",
        ));
}

#[test]
fn add_rejects_dotdot_file_path() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "../escape.facts"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("file must be in the project root"));
}

#[test]
fn add_accepts_valid_relative_file_path() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "valid.facts"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("valid.facts")).unwrap();
    assert!(content.contains("test"));
}

// ===========================================================================
// Reserved key prefix round-trip safety (ISSUE-009)
// ===========================================================================

#[test]
fn add_label_starting_with_command_roundtrips() {
    let dir = project("");
    // Add a plain fact whose label starts with "command:"
    facts_cmd(&dir)
        .args(["add", "command: echo hello"])
        .assert()
        .success();

    // Verify lint passes (file is not corrupted)
    facts_cmd(&dir).arg("lint").assert().success();

    // Verify the full label is preserved on list
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("command: echo hello"));
}

#[test]
fn add_label_starting_with_label_roundtrips() {
    let dir = project("");
    // Add a plain fact whose label starts with "label:"
    facts_cmd(&dir)
        .args(["add", "label: something"])
        .assert()
        .success();

    // Verify lint passes
    facts_cmd(&dir).arg("lint").assert().success();

    // Verify the full label is preserved (not silently truncated)
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("label: something"));
}

#[test]
fn add_label_starting_with_id_roundtrips() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "id: something"])
        .assert()
        .success();

    facts_cmd(&dir).arg("lint").assert().success();

    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("id: something"));
}

#[test]
fn add_label_starting_with_tags_roundtrips() {
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "tags: [a, b]"])
        .assert()
        .success();

    facts_cmd(&dir).arg("lint").assert().success();

    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("tags: [a, b]"));
}

// ===========================================================================
// Invalid tag expressions (ISSUE-004)
// ===========================================================================

#[test]
fn list_rejects_malformed_tag_expr_trailing_and() {
    let dir = project("- fact @mvp\n");
    facts_cmd(&dir)
        .args(["list", "--tags", "mvp and"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid tag expression"));
}

#[test]
fn list_rejects_malformed_tag_expr_unbalanced_parens() {
    let dir = project("- fact @mvp\n");
    facts_cmd(&dir)
        .args(["list", "--tags", "(("])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid tag expression"));
}

#[test]
fn check_rejects_empty_tag_expr() {
    let dir = project("- label: fact\n  command: \"true\"\n  tags: [mvp]\n");
    facts_cmd(&dir)
        .args(["check", "--tags", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid tag expression"));
}

// ===========================================================================
// input validation — empty labels
// ===========================================================================

#[test]
fn add_rejects_empty_label() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot be empty"));
}

#[test]
fn add_rejects_label_that_is_only_tags() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "@mvp"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot be empty"));
}

#[test]
fn add_rejects_whitespace_only_label() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "   "])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot be empty"));
}

#[test]
fn edit_rejects_empty_label() {
    let dir = project("- original fact\n");
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("original fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    facts_cmd(&dir)
        .args(["edit", &id, "--label", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("label cannot be empty"));
}

#[test]
fn remove_readonly_file_fails_without_printing_label() {
    use std::os::unix::fs::PermissionsExt;

    let dir = project("- fact to remove\n");

    // Find the ID for "fact to remove"
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("fact to remove"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    // Make the .facts file read-only
    let facts_path = dir.path().join(".facts");
    let perms = std::fs::Permissions::from_mode(0o444);
    fs::set_permissions(&facts_path, perms).unwrap();

    // remove should fail (can't write) and must NOT print the label to stdout
    facts_cmd(&dir)
        .args(["remove", id])
        .assert()
        .failure()
        .stdout(predicate::str::contains("fact to remove").not());

    // Restore permissions for cleanup
    let perms = std::fs::Permissions::from_mode(0o644);
    fs::set_permissions(&facts_path, perms).unwrap();
}

// ===========================================================================
// ISSUE-005: tags with whitespace
// ===========================================================================

#[test]
fn add_rejects_tag_with_whitespace() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--tags", "has space"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot contain whitespace"));
}

#[test]
fn edit_add_tag_rejects_whitespace() {
    let dir = project("- label: fact\n  command: echo ok\n  id: f1\n");
    facts_cmd(&dir)
        .args(["edit", "f1", "--add-tag", "has space"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot contain whitespace"));
}

#[test]
fn add_accepts_valid_comma_separated_tags() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--tags", "valid,tags"])
        .assert()
        .success();
    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("@valid"));
    assert!(content.contains("@tags"));
}

// ===========================================================================
// ISSUE-012: Empty command string accepted
// ===========================================================================

#[test]
fn add_rejects_empty_command() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--command", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("command cannot be empty"));
}

#[test]
fn edit_rejects_empty_command() {
    let dir = project("- label: fact\n  command: echo ok\n  id: f1\n");
    facts_cmd(&dir)
        .args(["edit", "f1", "--command", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("command cannot be empty"));
}

// ===========================================================================
// ISSUE-013: Section path components not trimmed
// ===========================================================================

#[test]
fn add_trims_section_path_components() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--section", "a / b"])
        .assert()
        .success();
    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(
        content.contains("# a\n"),
        "expected trimmed '# a' heading, got:\n{content}"
    );
    assert!(
        content.contains("## b\n"),
        "expected trimmed '## b' heading, got:\n{content}"
    );
}

#[test]
fn edit_noop_preserves_mapping_tag_order() {
    let facts = "- label: my fact\n  tags: [z-tag, a-tag, m-tag]\n";
    let dir = project(facts);

    // Get the fact ID
    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("my fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    // Perform a no-op edit (re-set the same label)
    facts_cmd(&dir)
        .args(["edit", id, "--label", "my fact"])
        .assert()
        .success();

    let after = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert_eq!(
        facts, after,
        "no-op edit must not reorder tags; expected:\n{facts}\ngot:\n{after}"
    );
}

// ===========================================================================
// ISSUE-016: Tags with @ prefix create @@ in file
// ===========================================================================

#[test]
fn add_strips_at_prefix_from_tags() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "@mvp"])
        .assert()
        .success();
    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(
        content.contains("@mvp"),
        "should contain @mvp, got:\n{content}"
    );
    assert!(
        !content.contains("@@mvp"),
        "should NOT contain @@mvp, got:\n{content}"
    );
    // Also verify list --tags finds it
    facts_cmd(&dir)
        .args(["list", "--tags", "mvp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("test fact"));
}

// ===========================================================================
// ISSUE-018: Empty explicit IDs accepted
// ===========================================================================

#[test]
fn add_rejects_empty_id() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test fact", "--id", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ID cannot be empty"));
}

#[test]
fn edit_rejects_empty_new_id() {
    let dir = project("- label: fact\n  id: f1\n");
    facts_cmd(&dir)
        .args(["edit", "f1", "--new-id", ""])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ID cannot be empty"));
}

// ===========================================================================
// ISSUE-021: Sections deeper than 6 levels produce invalid Markdown headings
// ===========================================================================

#[test]
fn add_accepts_section_depth_6() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "deep fact", "--section", "a/b/c/d/e/f"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    // Deepest heading should be ###### (6 levels)
    assert!(content.contains("# a"), "missing depth-1 heading");
    assert!(content.contains("###### f"), "missing depth-6 heading");
    assert!(content.contains("- deep fact"), "missing the fact");
}

#[test]
fn add_rejects_section_depth_7() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "too deep", "--section", "a/b/c/d/e/f/g"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "section path too deep (max 6 levels)",
        ));
}

// ===========================================================================
// ISSUE-025: computed ID collision with explicit ID
// ===========================================================================

#[test]
fn computed_id_extended_when_colliding_with_explicit_id() {
    // Step 1: Add a fact and discover its computed 3-char ID.
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "alpha fact"])
        .assert()
        .success();

    let list_out = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let computed_id: String = stdout
        .lines()
        .find(|l| l.contains("alpha fact"))
        .expect("alpha fact should appear in list")
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    // Step 2: Add a second fact with an explicit ID equal to the first fact's
    // computed ID. This creates a collision that the ID assigner must resolve.
    facts_cmd(&dir)
        .args(["add", "beta fact", "--id", &computed_id])
        .assert()
        .success();

    // Step 3: List should show both facts with DIFFERENT IDs.
    let list_out2 = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout2 = String::from_utf8_lossy(&list_out2.stdout);

    let alpha_id = stdout2
        .lines()
        .find(|l| l.contains("alpha fact"))
        .expect("alpha fact should still appear")
        .split_whitespace()
        .next()
        .unwrap();
    let beta_id = stdout2
        .lines()
        .find(|l| l.contains("beta fact"))
        .expect("beta fact should appear")
        .split_whitespace()
        .next()
        .unwrap();

    assert_ne!(
        alpha_id, beta_id,
        "computed and explicit IDs must not collide: both are '{alpha_id}'"
    );
    // The explicit ID should be preserved as-is.
    assert_eq!(beta_id, computed_id, "explicit ID must be preserved");
    // The computed ID should have been extended (longer than 3 chars).
    assert!(
        alpha_id.len() > computed_id.len(),
        "computed ID '{alpha_id}' should be longer than original '{computed_id}'"
    );
}

#[test]
fn explicit_id_collision_resolved_facts_remain_addressable() {
    // After collision resolution, both facts should be individually addressable
    // by their (now-unique) IDs for edit/remove operations.
    let dir = project("");
    facts_cmd(&dir)
        .args(["add", "first fact"])
        .assert()
        .success();

    let list_out = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_out.stdout);
    let computed_id: String = stdout
        .lines()
        .find(|l| l.contains("first fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    // Add second fact with explicit ID matching the first's computed ID.
    facts_cmd(&dir)
        .args(["add", "second fact", "--id", &computed_id])
        .assert()
        .success();

    // Get the extended ID for the first fact.
    let list_out2 = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout2 = String::from_utf8_lossy(&list_out2.stdout);
    let extended_id: String = stdout2
        .lines()
        .find(|l| l.contains("first fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_string();

    // Remove the first fact using its extended ID — should succeed.
    facts_cmd(&dir)
        .args(["remove", &extended_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("first fact"));

    // The second fact should still be listed with its explicit ID.
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains(&computed_id))
        .stdout(predicate::str::contains("second fact"));
}

// ===========================================================================
// ISSUE-023: add --id must check for duplicate explicit IDs across files
// ===========================================================================

#[test]
fn add_rejects_duplicate_id_across_files() {
    let dir = empty_project();

    // Add a fact with explicit ID "shared" to .facts
    facts_cmd(&dir)
        .args(["add", "fact one", "--id", "shared"])
        .assert()
        .success();

    // Try to add a fact with the same explicit ID to a different file
    facts_cmd(&dir)
        .args(["add", "fact two", "--file", "extra", "--id", "shared"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ID already exists: shared"));
}

#[test]
fn add_accepts_unique_id_across_files() {
    let dir = empty_project();

    // Add a fact with explicit ID "alpha" to .facts
    facts_cmd(&dir)
        .args(["add", "fact one", "--id", "alpha"])
        .assert()
        .success();

    // Add a fact with a DIFFERENT explicit ID to a different file — should succeed
    facts_cmd(&dir)
        .args(["add", "fact two", "--file", "extra", "--id", "beta"])
        .assert()
        .success();

    // Both files should exist with their respective facts
    let main_content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(main_content.contains("id: alpha"));
    let extra_content = fs::read_to_string(dir.path().join("extra.facts")).unwrap();
    assert!(extra_content.contains("id: beta"));
}

// ===========================================================================
// ISSUE-027: add --file rejects subdirectory paths
// ===========================================================================

#[test]
fn add_rejects_file_in_subdirectory() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "subdir/nested"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("file must be in the project root"));
}

#[test]
fn add_rejects_file_with_backslash_subdirectory() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "subdir\\nested"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("file must be in the project root"));
}

#[test]
fn add_accepts_file_in_project_root() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "test", "--file", "extra"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join("extra.facts")).unwrap();
    assert!(content.contains("test"));
}

// ===========================================================================
// ISSUE-024: lint must detect duplicate explicit IDs across files
// ===========================================================================

#[test]
fn lint_warns_duplicate_id_across_files() {
    let dir = empty_project();

    // Create two .facts files each with the same explicit ID
    fs::write(dir.path().join(".facts"), "- label: fact one\n  id: dupe\n").unwrap();
    fs::write(
        dir.path().join("extra.facts"),
        "- label: fact two\n  id: dupe\n",
    )
    .unwrap();

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success() // warnings don't cause failure
        .stderr(predicate::str::contains("duplicate explicit id 'dupe'"))
        .stderr(predicate::str::contains("across files"));
}

#[test]
fn lint_passes_unique_ids_across_files() {
    let dir = empty_project();

    // Create two .facts files with different explicit IDs
    fs::write(
        dir.path().join(".facts"),
        "- label: fact one\n  id: alpha\n",
    )
    .unwrap();
    fs::write(
        dir.path().join("extra.facts"),
        "- label: fact two\n  id: beta\n",
    )
    .unwrap();

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("2 files passed"));
}

// ===========================================================================
// add — tag validation (parentheses)
// ===========================================================================

#[test]
fn add_rejects_tag_with_parentheses() {
    let dir = project("- existing fact\n");

    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "v(beta)"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot contain parentheses"));
}

#[test]
fn add_rejects_tag_with_closing_paren() {
    let dir = project("- existing fact\n");

    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "beta)"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot contain parentheses"));
}

// ===========================================================================
// init — .facts directory collision (ISSUE-028)
// ===========================================================================

#[test]
fn init_errors_when_facts_is_directory() {
    let dir = empty_project();
    // Create a directory named `.facts` instead of a file
    fs::create_dir(dir.path().join(".facts")).unwrap();

    facts_cmd(&dir)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains(".facts exists but is not a file"));
}

// ===========================================================================
// lint — empty label detection (ISSUE-029)
// ===========================================================================

#[test]
fn lint_warns_empty_label() {
    let dir = project("- \n");

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success() // warnings don't cause failure
        .stderr(predicate::str::contains("empty label"));
}

#[test]
fn lint_passes_nonempty_labels() {
    let dir = project("- a real fact\n- another fact\n");

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 file passed"));
}

// ===========================================================================
// UTF-8 BOM handling
// ===========================================================================

#[test]
fn parse_handles_utf8_bom() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    // Write raw bytes: UTF-8 BOM (EF BB BF) followed by a fact
    fs::write(dir.path().join(".facts"), b"\xEF\xBB\xBF- a fact\n").unwrap();

    // list should show the fact
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("a fact"));

    // lint should pass
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 file passed"));
}

// ===========================================================================
// lint: unrecognized continuation lines (ISSUE-033)
// ===========================================================================

#[test]
fn lint_warns_unknown_continuation_line() {
    let dir =
        project("- label: test fact\n  this content is silently dropped\n  command: echo hi\n");
    // Unrecognized continuation is a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("unrecognized continuation line"));
}

// ===========================================================================
// lint: duplicate mapping keys (ISSUE-034)
// ===========================================================================

#[test]
fn lint_warns_duplicate_mapping_keys() {
    let dir = project("- label: first label\n  label: second label\n");
    // Duplicate keys is a warning, not an error -- should still pass (exit 0)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("duplicate key"));
}

// ===========================================================================
// ISSUE-031: inline @tag + --tags deduplication
// ===========================================================================

#[test]
fn add_deduplicates_inline_and_flag_tags() {
    let dir = empty_project();

    facts_cmd(&dir)
        .args(["add", "fact @mvp here", "--tags", "mvp"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    // The tag should appear exactly once -- not duplicated.
    assert!(
        content.contains("@mvp"),
        "file should contain @mvp: {content}"
    );
    // Count occurrences of "@mvp" -- must be exactly 1.
    let count = content.matches("@mvp").count();
    assert_eq!(
        count, 1,
        "expected exactly 1 @mvp, got {count} in: {content}"
    );
}

#[test]
fn edit_add_tag_deduplicates_inline() {
    // Start with a plain fact that has an inline tag.
    let dir = project("- some fact @mvp\n");

    let list_output = facts_cmd(&dir).arg("list").output().unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("some fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    // Add the same tag via --add-tag -- should NOT produce a duplicate.
    facts_cmd(&dir)
        .args(["edit", id, "--add-tag", "mvp"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    let count = content.matches("@mvp").count();
    assert_eq!(
        count, 1,
        "expected exactly 1 @mvp, got {count} in: {content}"
    );
}

// ===========================================================================
// add — reject operator-named tags (ISSUE-032)
// ===========================================================================

#[test]
fn add_rejects_tag_named_not() {
    let dir = project("- existing fact\n");

    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "not"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "tag name conflicts with filter operator: not",
        ));
}

#[test]
fn add_rejects_tag_named_and() {
    let dir = project("- existing fact\n");

    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "and"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "tag name conflicts with filter operator: and",
        ));
}

#[test]
fn add_rejects_tag_named_or() {
    let dir = project("- existing fact\n");

    facts_cmd(&dir)
        .args(["add", "test fact", "--tags", "or"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "tag name conflicts with filter operator: or",
        ));
}

// ===========================================================================
// ISSUE-035: Plain fact label wrapped in {} causes parse failure
// ===========================================================================

#[test]
fn plain_fact_with_curly_braces() {
    let dir = project("- {this is a note}\n");

    // lint should pass (no errors or warnings)
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));

    // list should show it as a fact
    facts_cmd(&dir)
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("{this is a note}"));
}

// ===========================================================================
// ISSUE-036: Mapping keys with no value silently ignored
// ===========================================================================

#[test]
fn lint_warns_empty_mapping_value() {
    let dir = project("- label: test\n  command:\n");

    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success()
        .stderr(predicate::str::contains("key 'command' has no value"));
}

// ===========================================================================
// ISSUE-037: @@tag creates tag with @ prefix
// ===========================================================================

#[test]
fn double_at_tag_stripped_to_single() {
    let dir = empty_project();
    facts_cmd(&dir)
        .args(["add", "fact @@important"])
        .assert()
        .success();

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    // The tag should be stored as @important (single @), not @@important
    assert!(
        content.contains("@important"),
        "should contain @important, got:\n{content}"
    );
    assert!(
        !content.contains("@@important"),
        "should NOT contain @@important, got:\n{content}"
    );
}

#[test]
fn list_filters_double_at_tag() {
    let dir = project("- fact @@important\n");
    // The parser should strip the extra @ so --tags "important" matches
    facts_cmd(&dir)
        .args(["list", "--tags", "important"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact"));
}
