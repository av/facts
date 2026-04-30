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
    let dir = project(
        "# alpha\n\n- alpha fact\n\n# beta\n\n- beta fact\n",
    );
    facts_cmd(&dir)
        .args(["list", "--section", "beta"])
        .assert()
        .success()
        .stdout(predicate::str::contains("beta fact"))
        .stdout(predicate::str::contains("alpha fact").not());
}

#[test]
fn list_filter_has_command() {
    let dir = project(
        "- manual fact\n- label: cmd fact\n  command: echo hi\n",
    );
    facts_cmd(&dir)
        .args(["list", "--has-command"])
        .assert()
        .success()
        .stdout(predicate::str::contains("cmd fact"))
        .stdout(predicate::str::contains("manual fact").not());
}

#[test]
fn list_filter_manual() {
    let dir = project(
        "- manual fact\n- label: cmd fact\n  command: echo hi\n",
    );
    facts_cmd(&dir)
        .args(["list", "--manual"])
        .assert()
        .success()
        .stdout(predicate::str::contains("manual fact"))
        .stdout(predicate::str::contains("cmd fact").not());
}

#[test]
fn list_filter_tags() {
    let dir = project(
        "- tagged fact @mvp\n- untagged fact\n",
    );
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
    let output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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

    let output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    let output = facts_cmd(&dir)
        .arg("check")
        .output()
        .unwrap();
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
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success();
}

#[test]
fn check_exit_code_nonzero_when_any_fail() {
    let dir = project("- label: nope\n  command: \"false\"\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .failure();
}

#[test]
fn check_exit_zero_with_only_manual_facts() {
    let dir = project("- just a manual fact\n");
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success();
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
    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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

    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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

    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    let dir = project(
        "# project\n\n- a valid fact\n- label: mapping fact\n  command: echo ok\n",
    );
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
        .stdout(predicate::str::contains("created"))
        .stdout(predicate::str::contains("Rust/Cargo"));

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("Cargo"));
}

#[test]
fn init_refuses_overwrite() {
    let dir = project("- existing\n");
    facts_cmd(&dir)
        .arg("init")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));

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
        .stdout(predicate::str::contains("no known frameworks"));

    let content = fs::read_to_string(dir.path().join(".facts")).unwrap();
    assert!(content.contains("# project"));
}

// ===========================================================================
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

    let output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&list_output.stdout);
    let id = stdout
        .lines()
        .find(|l| l.contains("ephemeral fact"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    facts_cmd(&dir)
        .args(["remove", id])
        .assert()
        .success();

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
    let dir = project(
        "- fact one @mvp @core\n- fact two @mvp\n- fact three @core\n",
    );
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
    let output = facts_cmd(&dir)
        .arg("check")
        .output()
        .unwrap();
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
    facts_cmd(&dir)
        .arg("lint")
        .assert()
        .success();
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

    let output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    let full_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    fs::write(
        dir.path().join("other.facts"),
        "- fact in other\n",
    )
    .unwrap();

    // Get ID from list
    let list_output = facts_cmd(&dir)
        .arg("list")
        .output()
        .unwrap();
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
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success();
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
    facts_cmd(&dir)
        .arg("check")
        .assert()
        .success();
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
    let dir = project(
        "# level1\n\n## level2\n\n### level3\n\n#### level4\n\n- deep fact\n",
    );
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
    let dir = project(
        "# l1\n\n## l2\n\n### l3\n\n- label: deep cmd\n  command: \"true\"\n",
    );
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
    let dir = project(
        "- fact a @mvp @core\n- fact b @mvp\n- fact c @core\n- fact d @blocked\n",
    );
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
    let dir = project(
        "- fact a @x @y\n- fact b @x @z\n- fact c @y @z\n",
    );
    // x and (y or z) — should match a (x+y), b (x+z), but not c (no x)
    facts_cmd(&dir)
        .args(["list", "--tags", "x and (y or z)"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fact a"))
        .stdout(predicate::str::contains("fact b"))
        .stdout(predicate::str::contains("fact c").not());
}
