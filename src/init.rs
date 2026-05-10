/// The `init` subcommand — scaffold a .facts file and install agent skills.
///
/// Detects well-known framework/runtime combos by checking for marker files,
/// reading config files, and inspecting declared dependencies in the project
/// root, then generates initial facts for each detected stack.
///
/// Skills are installed to `.agents/skills/` (agent-agnostic). If Claude Code
/// is detected on the system, symlinks are created from `.claude/skills/`.
///
/// Idempotent: skips steps that are already done.
use anyhow::Result;
use std::path::Path;

use crate::project;

// Embedded skill content, baked into the binary at compile time.
const FACTS_SKILL: &str = include_str!("../skills/facts/SKILL.md");
const DISCOVER_SKILL: &str = include_str!("../skills/facts-discover/SKILL.md");
const IMPLEMENT_SKILL: &str = include_str!("../skills/facts-implement/SKILL.md");
const REFINE_SKILL: &str = include_str!("../skills/facts-refine/SKILL.md");

pub(crate) const SKILLS: &[(&str, &str)] = &[
    ("facts", FACTS_SKILL),
    ("facts-discover", DISCOVER_SKILL),
    ("facts-implement", IMPLEMENT_SKILL),
    ("facts-refine", REFINE_SKILL),
];

const SECTION_START: &str = "<!-- facts:start -->";
const SECTION_END: &str = "<!-- facts:end -->";

const AGENT_DOCS_SECTION: &str = "\
<!-- facts:start -->
## Fact-driven development

This project uses [facts](https://github.com/av/facts) for specification and documentation. All work flows through the fact sheet — it is the source of truth.

**Every change starts with a fact.** Facts are the spec — they define what \"done\" means. Code that isn't described by a fact is unverifiable and will be treated as incorrect. The skill `facts skills show facts` has the full format spec and command reference.

1. `facts list` — read the current spec to orient. Fact sheets can be large — use filters to focus: `--section \"cli/init\"`, `--tags \"draft\"`, `--file api.facts`, `--manual`. Read only the section relevant to your task, not the entire sheet.
2. `facts add` — write facts describing what should be true when done. Each fact is a testable claim. You are not ready to write code until this step is complete.
3. Implement the code to make those facts true
4. `facts check --tags \"<tag>\"` or `facts get <id>` — verify your changes. Never run bare `facts check` unless asked.
5. `facts edit <id> --add-tag implemented` — mark verified facts done

Step 4 only works if step 2 happened. If you skipped step 2, go back now — you cannot verify work that has no fact.

**Manual facts (`?` in check output):** these have no command, so you verify them by reading the relevant code. For each `?` fact: read what it claims, check the code, report PASS or FAIL with a one-line reason. Reporting \"N manual\" without verifying each one is not acceptable.

**Lifecycle:** `@draft` → `@spec` → `@implemented`

**Skills** (invoke via `facts skills show <name>`):
- `facts-refine` — sharpen `@draft` facts into `@spec` with the user
- `facts-discover` — scan the codebase and sync facts to reality (only when explicitly asked)
- `facts-implement` — implement `@spec` facts in code, verify, tag `@implemented`
<!-- facts:end -->";

const AGENT_MD_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md"];

/// A detected project stack.
#[derive(Debug, PartialEq)]
pub struct DetectedStack {
    pub name: &'static str,
    pub marker: &'static str,
    pub facts: Vec<StackFact>,
}

/// A fact to scaffold for a detected stack.
#[derive(Debug, PartialEq)]
pub struct StackFact {
    pub label: String,
    pub command: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub fn run() -> Result<()> {
    let root = project::find_project_root()?;
    run_in(&root)
}

fn run_in(root: &Path) -> Result<()> {
    let facts_path = root.join(".facts");

    if facts_path.exists() && !facts_path.is_file() {
        anyhow::bail!(".facts exists but is not a file");
    } else if facts_path.is_file() {
        println!("  skip  .facts (already exists)");
    } else {
        let stacks = detect_stacks(root);
        let content = generate_facts_content(&stacks);
        std::fs::write(&facts_path, &content)?;
        if stacks.is_empty() {
            println!("  create  .facts (no frameworks detected)");
        } else {
            let names: Vec<&str> = stacks.iter().map(|s| s.name).collect();
            println!("  create  .facts (detected: {})", names.join(", "));
        }
    }

    for (name, content) in SKILLS {
        install_skill(root, name, content)?;
    }

    if is_claude_available(root) {
        for (name, _) in SKILLS {
            link_skill_for_claude(root, name)?;
        }
    }

    install_agent_docs(root)?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Skill installation
// ---------------------------------------------------------------------------

pub(crate) fn install_skill(root: &Path, name: &str, content: &str) -> Result<()> {
    let skill_dir = root.join(".agents").join("skills").join(name);
    let skill_path = skill_dir.join("SKILL.md");

    if skill_path.exists() {
        let existing = std::fs::read_to_string(&skill_path)?;
        if existing == content {
            println!("  skip  .agents/skills/{name}/SKILL.md (up to date)");
        } else {
            std::fs::write(&skill_path, content)?;
            println!("  update  .agents/skills/{name}/SKILL.md");
        }
    } else {
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(&skill_path, content)?;
        println!("  create  .agents/skills/{name}/SKILL.md");
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Claude symlinks
// ---------------------------------------------------------------------------

pub(crate) fn is_claude_available(root: &Path) -> bool {
    if root.join(".claude").exists() {
        return true;
    }
    if let Ok(home) = std::env::var("HOME")
        && Path::new(&home).join(".claude").exists()
    {
        return true;
    }
    std::process::Command::new("which")
        .arg("claude")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(unix)]
pub(crate) fn link_skill_for_claude(root: &Path, name: &str) -> Result<()> {
    let link_dir = root.join(".claude").join("skills");
    let link_path = link_dir.join(name);
    // Relative from .claude/skills/ up to project root, then into .agents/skills/<name>
    let target = Path::new("..")
        .join("..")
        .join(".agents")
        .join("skills")
        .join(name);

    if link_path.is_symlink() {
        let current = std::fs::read_link(&link_path)?;
        if current == target {
            println!("  skip  .claude/skills/{name} (link up to date)");
            return Ok(());
        }
        std::fs::remove_file(&link_path)?;
    } else if link_path.exists() {
        // Real dir/file — don't overwrite user content.
        println!("  skip  .claude/skills/{name} (exists, not a symlink)");
        return Ok(());
    }

    std::fs::create_dir_all(&link_dir)?;
    std::os::unix::fs::symlink(&target, &link_path)?;
    println!("  link  .claude/skills/{name} -> .agents/skills/{name}");
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn link_skill_for_claude(_root: &Path, name: &str) -> Result<()> {
    println!("  skip  .claude/skills/{name} (symlinks not supported on this platform)");
    Ok(())
}

// ---------------------------------------------------------------------------
// Agent docs (CLAUDE.md / AGENTS.md)
// ---------------------------------------------------------------------------

fn install_agent_docs(root: &Path) -> Result<()> {
    let mut installed = false;

    for name in AGENT_MD_FILES {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        if content.contains(SECTION_START) {
            let start = content.find(SECTION_START).unwrap();
            let end_marker = content[start..].find(SECTION_END).unwrap();
            let end = start + end_marker + SECTION_END.len();
            let existing_section = &content[start..end];
            if existing_section == AGENT_DOCS_SECTION {
                println!("  skip  {name} (facts section up to date)");
            } else {
                let mut new_content = String::new();
                new_content.push_str(&content[..start]);
                new_content.push_str(AGENT_DOCS_SECTION);
                new_content.push_str(&content[end..]);
                std::fs::write(&path, new_content)?;
                println!("  update  {name} (facts section updated)");
            }
        } else {
            let mut new_content = content.clone();
            if !new_content.ends_with('\n') && !new_content.is_empty() {
                new_content.push('\n');
            }
            if !new_content.is_empty() {
                new_content.push('\n');
            }
            new_content.push_str(AGENT_DOCS_SECTION);
            new_content.push('\n');
            std::fs::write(&path, new_content)?;
            println!("  update  {name} (added facts section)");
        }
        installed = true;
    }

    if !installed {
        let name = AGENT_MD_FILES.last().unwrap();
        let path = root.join(name);
        let mut content = String::from(AGENT_DOCS_SECTION);
        content.push('\n');
        std::fs::write(&path, content)?;
        println!("  create  {name} (with facts section)");
    }

    Ok(())
}

pub fn remove_agent_docs(root: &Path) -> Result<()> {
    for name in AGENT_MD_FILES {
        let path = root.join(name);
        if !path.is_file() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let Some(start) = content.find(SECTION_START) else {
            continue;
        };
        let Some(end_marker) = content[start..].find(SECTION_END) else {
            continue;
        };
        let end = start + end_marker + SECTION_END.len();

        let before = content[..start].trim_end_matches('\n');
        let after = content[end..].trim_start_matches('\n');

        let new_content = if before.is_empty() && after.is_empty() {
            String::new()
        } else if before.is_empty() {
            format!("{after}\n")
        } else if after.is_empty() {
            format!("{before}\n")
        } else {
            format!("{before}\n\n{after}\n")
        };

        if new_content.is_empty() {
            std::fs::remove_file(&path)?;
            println!("  remove  {name} (was only facts section)");
        } else {
            std::fs::write(&path, new_content)?;
            println!("  update  {name} (removed facts section)");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Stack detection
// ---------------------------------------------------------------------------

pub fn detect_stacks(root: &Path) -> Vec<DetectedStack> {
    let detectors: &[fn(&Path) -> Option<DetectedStack>] = &[
        detect_deno,
        detect_node,
        detect_rust,
        detect_python,
        detect_go,
        detect_ruby,
        detect_java_maven,
        detect_java_gradle,
        detect_elixir,
        detect_php,
        detect_swift,
        detect_dotnet,
        detect_zig,
        detect_gleam,
        detect_dart,
        detect_docker,
        detect_terraform,
    ];

    detectors.iter().filter_map(|d| d(root)).collect()
}

// ---------------------------------------------------------------------------
// Individual detectors
// ---------------------------------------------------------------------------

fn detect_deno(root: &Path) -> Option<DetectedStack> {
    let config_name = if file_exists(root, "deno.json") {
        "deno.json"
    } else if file_exists(root, "deno.jsonc") {
        "deno.jsonc"
    } else {
        return None;
    };

    let content = read_file(root, config_name).unwrap_or_default();
    let mut facts = Vec::new();

    if json_section_has_key(&content, "tasks", "build") {
        facts.push(sfact("project builds successfully", "deno task build"));
    }
    if json_section_has_key(&content, "tasks", "test") {
        facts.push(sfact("all tests pass", "deno task test"));
    } else {
        facts.push(sfact("all tests pass", "deno test"));
    }
    if json_section_has_key(&content, "tasks", "lint") {
        facts.push(sfact("code passes linting", "deno task lint"));
    } else {
        facts.push(sfact("code passes linting", "deno lint"));
    }
    facts.push(sfact("code is formatted", "deno fmt --check"));

    Some(DetectedStack {
        name: "Deno",
        marker: "deno.json",
        facts,
    })
}

fn detect_node(root: &Path) -> Option<DetectedStack> {
    let content = read_file(root, "package.json")?;

    // Deno projects may also have package.json — defer to the Deno detector.
    if file_exists(root, "deno.json") || file_exists(root, "deno.jsonc") {
        return None;
    }

    let pm = detect_js_pm(root);

    let name = if json_has_dep(&content, "next") {
        "Next.js"
    } else if json_has_dep(&content, "nuxt") {
        "Nuxt"
    } else if json_has_dep(&content, "@sveltejs/kit") {
        "SvelteKit"
    } else if json_has_dep(&content, "@remix-run/node") || json_has_dep(&content, "remix") {
        "Remix"
    } else {
        "Node.js"
    };

    let mut facts = Vec::new();

    // Build
    if json_has_script(&content, "build") {
        facts.push(sfact("project builds successfully", &pm.run("build")));
    }

    // Test: script first, then dep fallback
    if json_has_script(&content, "test") {
        facts.push(sfact("all tests pass", pm.test()));
    } else if json_has_dep(&content, "vitest") {
        facts.push(sfact("all tests pass", &pm.exec("vitest run")));
    } else if json_has_dep(&content, "jest") {
        facts.push(sfact("all tests pass", &pm.exec("jest")));
    } else if json_has_dep(&content, "mocha") {
        facts.push(sfact("all tests pass", &pm.exec("mocha")));
    }

    // Lint: script first, then dep fallback
    if json_has_script(&content, "lint") {
        facts.push(sfact("code passes linting", &pm.run("lint")));
    } else if json_has_dep(&content, "@biomejs/biome") {
        facts.push(sfact("code passes linting", &pm.exec("biome check")));
    } else if json_has_dep(&content, "eslint") {
        facts.push(sfact("code passes linting", &pm.exec("eslint .")));
    }

    // Typecheck: script first, then dep fallback
    if json_has_dep(&content, "typescript") {
        if json_has_script(&content, "typecheck") {
            facts.push(sfact("type checking passes", &pm.run("typecheck")));
        } else if json_has_script(&content, "type-check") {
            facts.push(sfact("type checking passes", &pm.run("type-check")));
        } else if json_has_script(&content, "types") {
            facts.push(sfact("type checking passes", &pm.run("types")));
        } else {
            facts.push(sfact("type checking passes", &pm.exec("tsc --noEmit")));
        }
    }

    // Format: script first, then dep fallback
    if json_has_script(&content, "format:check") {
        facts.push(sfact("code is formatted", &pm.run("format:check")));
    } else if json_has_dep(&content, "prettier") {
        facts.push(sfact("code is formatted", &pm.exec("prettier --check .")));
    }

    if facts.is_empty() {
        facts.push(StackFact {
            label: format!("project uses {name}"),
            command: Some("test -f package.json".into()),
        });
    }

    Some(DetectedStack {
        name,
        marker: "package.json",
        facts,
    })
}

fn detect_rust(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "Cargo.toml") {
        return None;
    }

    let mut facts = vec![
        sfact("project compiles successfully", "cargo check"),
        sfact("all tests pass", "cargo test --quiet"),
        sfact("code is formatted", "cargo fmt --check"),
    ];

    if file_exists(root, "clippy.toml") || file_exists(root, ".clippy.toml") {
        facts.push(sfact("clippy passes", "cargo clippy -- -D warnings"));
    }

    Some(DetectedStack {
        name: "Rust/Cargo",
        marker: "Cargo.toml",
        facts,
    })
}

fn detect_python(root: &Path) -> Option<DetectedStack> {
    if let Some(pyproject) = read_file(root, "pyproject.toml") {
        return Some(detect_python_pyproject(root, &pyproject));
    }

    if file_exists(root, "requirements.txt") {
        return Some(detect_python_requirements(root));
    }

    None
}

fn detect_python_pyproject(root: &Path, pyproject: &str) -> DetectedStack {
    let runner = detect_py_runner(root, pyproject);
    let mut facts = Vec::new();

    // Test: config first, then dep fallback
    if toml_has_section(pyproject, "tool.pytest")
        || file_exists(root, "conftest.py")
        || file_exists(root, "pytest.ini")
        || dep_in_pyproject(pyproject, "pytest")
    {
        facts.push(sfact("all tests pass", &runner.cmd("pytest --quiet")));
    }

    // Lint: config first, then dep fallback
    let has_ruff = toml_has_section(pyproject, "tool.ruff")
        || file_exists(root, "ruff.toml")
        || file_exists(root, ".ruff.toml")
        || dep_in_pyproject(pyproject, "ruff");

    if has_ruff {
        facts.push(sfact("code passes linting", &runner.cmd("ruff check .")));

        if toml_has_section(pyproject, "tool.ruff.format") {
            facts.push(sfact(
                "code is formatted",
                &runner.cmd("ruff format --check ."),
            ));
        }
    } else if toml_has_section(pyproject, "tool.flake8") || dep_in_pyproject(pyproject, "flake8") {
        facts.push(sfact("code passes linting", &runner.cmd("flake8")));
    }

    // Type checking: config first, then dep fallback
    if toml_has_section(pyproject, "tool.mypy")
        || file_exists(root, "mypy.ini")
        || dep_in_pyproject(pyproject, "mypy")
    {
        facts.push(sfact("type checking passes", &runner.cmd("mypy .")));
    } else if toml_has_section(pyproject, "tool.pyright")
        || file_exists(root, "pyrightconfig.json")
        || dep_in_pyproject(pyproject, "pyright")
    {
        facts.push(sfact("type checking passes", &runner.cmd("pyright")));
    }

    // Formatting (black, only if ruff format not already added)
    if !toml_has_section(pyproject, "tool.ruff.format")
        && (toml_has_section(pyproject, "tool.black") || dep_in_pyproject(pyproject, "black"))
    {
        facts.push(sfact("code is formatted", &runner.cmd("black --check .")));
    }

    if facts.is_empty() {
        facts.push(StackFact {
            label: "project uses Python".into(),
            command: Some("test -f pyproject.toml".into()),
        });
    }

    DetectedStack {
        name: "Python (pyproject.toml)",
        marker: "pyproject.toml",
        facts,
    }
}

fn detect_python_requirements(root: &Path) -> DetectedStack {
    let reqs = read_file(root, "requirements.txt").unwrap_or_default();
    let mut facts = Vec::new();

    // Test
    if file_exists(root, "conftest.py")
        || file_exists(root, "pytest.ini")
        || dep_in_requirements(&reqs, "pytest")
    {
        facts.push(sfact("all tests pass", "python -m pytest --quiet"));
    }

    // Lint
    if file_exists(root, "ruff.toml")
        || file_exists(root, ".ruff.toml")
        || dep_in_requirements(&reqs, "ruff")
    {
        facts.push(sfact("code passes linting", "ruff check ."));
    } else if dep_in_requirements(&reqs, "flake8") {
        facts.push(sfact("code passes linting", "flake8"));
    }

    // Type checking
    if file_exists(root, "mypy.ini")
        || file_exists(root, ".mypy.ini")
        || dep_in_requirements(&reqs, "mypy")
    {
        facts.push(sfact("type checking passes", "mypy ."));
    } else if dep_in_requirements(&reqs, "pyright") {
        facts.push(sfact("type checking passes", "pyright"));
    }

    // Format
    if dep_in_requirements(&reqs, "black") {
        facts.push(sfact("code is formatted", "black --check ."));
    }

    if facts.is_empty() {
        facts.push(StackFact {
            label: "project uses Python".into(),
            command: Some("test -f requirements.txt".into()),
        });
    }

    DetectedStack {
        name: "Python (requirements.txt)",
        marker: "requirements.txt",
        facts,
    }
}

fn detect_go(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "go.mod") {
        return None;
    }

    let mut facts = vec![
        sfact("project builds successfully", "go build ./..."),
        sfact("all tests pass", "go test ./..."),
        sfact("code is formatted", "test -z \"$(gofmt -l .)\""),
    ];

    if file_exists(root, ".golangci.yml")
        || file_exists(root, ".golangci.yaml")
        || file_exists(root, ".golangci.toml")
    {
        facts.push(sfact("linter passes", "golangci-lint run"));
    }

    Some(DetectedStack {
        name: "Go",
        marker: "go.mod",
        facts,
    })
}

fn detect_ruby(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "Gemfile") {
        return None;
    }

    let mut facts = vec![sfact("dependencies are installed", "bundle check")];

    if file_exists(root, "Rakefile") {
        facts.push(sfact("all tests pass", "bundle exec rake test"));
    } else if root.join("spec").is_dir() {
        facts.push(sfact("all tests pass", "bundle exec rspec"));
    }

    if file_exists(root, ".rubocop.yml") {
        facts.push(sfact("code passes linting", "bundle exec rubocop"));
    }

    Some(DetectedStack {
        name: "Ruby",
        marker: "Gemfile",
        facts,
    })
}

fn detect_java_maven(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "pom.xml") {
        return None;
    }
    Some(DetectedStack {
        name: "Java (Maven)",
        marker: "pom.xml",
        facts: vec![
            sfact("project compiles successfully", "mvn compile -q"),
            sfact("all tests pass", "mvn test -q"),
        ],
    })
}

fn detect_java_gradle(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "build.gradle") && !file_exists(root, "build.gradle.kts") {
        return None;
    }
    Some(DetectedStack {
        name: "Java (Gradle)",
        marker: "build.gradle",
        facts: vec![
            sfact(
                "project compiles successfully",
                "./gradlew compileJava --quiet",
            ),
            sfact("all tests pass", "./gradlew test --quiet"),
        ],
    })
}

fn detect_elixir(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "mix.exs") {
        return None;
    }
    Some(DetectedStack {
        name: "Elixir",
        marker: "mix.exs",
        facts: vec![
            sfact(
                "project compiles successfully",
                "mix compile --warnings-as-errors",
            ),
            sfact("all tests pass", "mix test"),
        ],
    })
}

fn detect_php(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "composer.json") {
        return None;
    }

    let content = read_file(root, "composer.json").unwrap_or_default();
    let mut facts = vec![sfact("dependencies are installed", "test -d vendor")];

    if json_has_script(&content, "test") {
        facts.push(sfact("all tests pass", "composer test"));
    } else if file_exists(root, "phpunit.xml") || file_exists(root, "phpunit.xml.dist") {
        facts.push(sfact("all tests pass", "./vendor/bin/phpunit"));
    }

    if json_has_script(&content, "lint") {
        facts.push(sfact("code passes linting", "composer lint"));
    }

    Some(DetectedStack {
        name: "PHP (Composer)",
        marker: "composer.json",
        facts,
    })
}

fn detect_swift(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "Package.swift") {
        return None;
    }
    Some(DetectedStack {
        name: "Swift",
        marker: "Package.swift",
        facts: vec![
            sfact("project builds successfully", "swift build"),
            sfact("all tests pass", "swift test"),
        ],
    })
}

fn detect_dotnet(root: &Path) -> Option<DetectedStack> {
    if !has_glob(root, ".csproj") {
        return None;
    }
    Some(DetectedStack {
        name: "C# (.NET)",
        marker: "*.csproj",
        facts: vec![
            sfact("project builds successfully", "dotnet build --nologo -q"),
            sfact("all tests pass", "dotnet test --nologo -q"),
        ],
    })
}

fn detect_zig(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "build.zig") {
        return None;
    }
    Some(DetectedStack {
        name: "Zig",
        marker: "build.zig",
        facts: vec![
            sfact("project builds successfully", "zig build"),
            sfact("all tests pass", "zig build test"),
        ],
    })
}

fn detect_gleam(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "gleam.toml") {
        return None;
    }
    Some(DetectedStack {
        name: "Gleam",
        marker: "gleam.toml",
        facts: vec![
            sfact("project builds successfully", "gleam build"),
            sfact("all tests pass", "gleam test"),
        ],
    })
}

fn detect_dart(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "pubspec.yaml") {
        return None;
    }

    let is_flutter = file_exists(root, "lib/main.dart")
        || read_file(root, "pubspec.yaml").is_some_and(|c| c.contains("flutter:"));

    if is_flutter {
        Some(DetectedStack {
            name: "Flutter",
            marker: "pubspec.yaml",
            facts: vec![
                sfact("all tests pass", "flutter test"),
                sfact("code passes analysis", "flutter analyze"),
            ],
        })
    } else {
        Some(DetectedStack {
            name: "Dart",
            marker: "pubspec.yaml",
            facts: vec![
                sfact("all tests pass", "dart test"),
                sfact("code passes analysis", "dart analyze"),
            ],
        })
    }
}

fn detect_docker(root: &Path) -> Option<DetectedStack> {
    if !file_exists(root, "Dockerfile") {
        return None;
    }
    Some(DetectedStack {
        name: "Docker",
        marker: "Dockerfile",
        facts: vec![sfact(
            "Docker image builds",
            "docker build -t facts-check .",
        )],
    })
}

fn detect_terraform(root: &Path) -> Option<DetectedStack> {
    if !has_glob(root, ".tf") {
        return None;
    }
    Some(DetectedStack {
        name: "Terraform",
        marker: "*.tf",
        facts: vec![
            sfact("Terraform configuration is valid", "terraform validate"),
            sfact("Terraform is formatted", "terraform fmt -check"),
        ],
    })
}

// ---------------------------------------------------------------------------
// Content generation
// ---------------------------------------------------------------------------

pub fn generate_facts_content(stacks: &[DetectedStack]) -> String {
    let mut out = String::new();

    if stacks.is_empty() {
        out.push_str("# project\n\n");
        out.push_str("- project is set up and ready for development\n");
        return out;
    }

    out.push_str("# project\n");

    for stack in stacks {
        out.push('\n');
        for fact in &stack.facts {
            if let Some(ref cmd) = fact.command {
                out.push_str(&format!("- label: {}\n  command: {}\n", fact.label, cmd));
            } else {
                out.push_str(&format!("- {}\n", fact.label));
            }
        }
    }

    out
}

// ---------------------------------------------------------------------------
// JS package manager detection
// ---------------------------------------------------------------------------

enum JsPm {
    Npm,
    Yarn,
    Pnpm,
    Bun,
}

impl JsPm {
    fn run(&self, script: &str) -> String {
        match self {
            JsPm::Npm => format!("npm run {script}"),
            JsPm::Yarn => format!("yarn {script}"),
            JsPm::Pnpm => format!("pnpm run {script}"),
            JsPm::Bun => format!("bun run {script}"),
        }
    }

    fn test(&self) -> &'static str {
        match self {
            JsPm::Npm => "npm test",
            JsPm::Yarn => "yarn test",
            JsPm::Pnpm => "pnpm test",
            JsPm::Bun => "bun test",
        }
    }

    fn exec(&self, bin: &str) -> String {
        match self {
            JsPm::Npm => format!("npx {bin}"),
            JsPm::Yarn => format!("yarn {bin}"),
            JsPm::Pnpm => format!("pnpm exec {bin}"),
            JsPm::Bun => format!("bunx {bin}"),
        }
    }
}

fn detect_js_pm(root: &Path) -> JsPm {
    if file_exists(root, "pnpm-lock.yaml") {
        JsPm::Pnpm
    } else if file_exists(root, "yarn.lock") {
        JsPm::Yarn
    } else if file_exists(root, "bun.lockb") || file_exists(root, "bun.lock") {
        JsPm::Bun
    } else {
        JsPm::Npm
    }
}

// ---------------------------------------------------------------------------
// Python runner detection
// ---------------------------------------------------------------------------

enum PyRunner {
    Poetry,
    Pdm,
    Uv,
    Direct,
}

impl PyRunner {
    fn cmd(&self, tool: &str) -> String {
        match self {
            PyRunner::Poetry => format!("poetry run {tool}"),
            PyRunner::Pdm => format!("pdm run {tool}"),
            PyRunner::Uv => format!("uv run {tool}"),
            PyRunner::Direct => tool.to_string(),
        }
    }
}

fn detect_py_runner(root: &Path, pyproject: &str) -> PyRunner {
    if file_exists(root, "poetry.lock") || toml_has_section(pyproject, "tool.poetry") {
        PyRunner::Poetry
    } else if file_exists(root, "pdm.lock") {
        PyRunner::Pdm
    } else if file_exists(root, "uv.lock") {
        PyRunner::Uv
    } else {
        PyRunner::Direct
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_file(root: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(root.join(name)).ok()
}

fn file_exists(root: &Path, name: &str) -> bool {
    root.join(name).exists()
}

fn has_glob(root: &Path, suffix: &str) -> bool {
    std::fs::read_dir(root)
        .ok()
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .any(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(suffix)))
        })
        .unwrap_or(false)
}

fn sfact(label: &str, command: &str) -> StackFact {
    StackFact {
        label: label.to_string(),
        command: Some(command.to_string()),
    }
}

/// Check if JSON content has a key within a named object section.
///
/// Scans for `"section": { ... }` and checks if `"key"` appears inside.
/// Handles nested braces and skips false matches (string values, nested keys).
fn json_section_has_key(content: &str, section: &str, key: &str) -> bool {
    let section_pat = format!("\"{}\"", section);
    let key_pat = format!("\"{}\"", key);

    let mut search_from = 0;
    while let Some(pos) = content[search_from..].find(&section_pat) {
        let abs_pos = search_from + pos;
        let after = &content[abs_pos + section_pat.len()..];

        let trimmed = after.trim_start();
        if !trimmed.starts_with(':') {
            search_from = abs_pos + section_pat.len();
            continue;
        }
        let after_colon = trimmed[1..].trim_start();

        if !after_colon.starts_with('{') {
            search_from = abs_pos + section_pat.len();
            continue;
        }

        // Found "section": { ... } — scan for matching brace.
        let mut depth = 0;
        for (i, ch) in after_colon.char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return after_colon[..i].contains(&key_pat);
                    }
                }
                _ => {}
            }
        }

        search_from = abs_pos + section_pat.len();
    }
    false
}

fn json_has_script(content: &str, script: &str) -> bool {
    json_section_has_key(content, "scripts", script)
}

fn json_has_dep(content: &str, dep: &str) -> bool {
    json_section_has_key(content, "dependencies", dep)
        || json_section_has_key(content, "devDependencies", dep)
}

/// Check if TOML content has a section header matching `[section]` or `[section.*]`.
fn toml_has_section(content: &str, section: &str) -> bool {
    let exact = format!("[{}]", section);
    let prefix = format!("[{}.", section);
    content.lines().any(|line| {
        let t = line.trim();
        t == exact || t.starts_with(&prefix)
    })
}

// ---------------------------------------------------------------------------
// Dependency detection helpers
// ---------------------------------------------------------------------------

/// Check if a dependency line matches a package name.
/// Handles version specifiers: `pkg`, `pkg>=1.0`, `pkg[extra]`, `pkg ==2`, etc.
fn dep_line_matches(line: &str, dep: &str) -> bool {
    if !line.starts_with(dep) {
        return false;
    }
    if line.len() == dep.len() {
        return true;
    }
    matches!(
        line.as_bytes()[dep.len()],
        b'>' | b'<' | b'=' | b'!' | b'~' | b'[' | b' ' | b';'
    )
}

/// Check if a pyproject.toml mentions a dependency anywhere in its dep arrays.
fn dep_in_pyproject(content: &str, dep: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line
            .trim()
            .trim_start_matches('-')
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches('"')
            .trim_matches('\'');
        dep_line_matches(trimmed, dep)
    })
}

/// Check if a requirements.txt file lists a dependency.
fn dep_in_requirements(content: &str, dep: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            return false;
        }
        dep_line_matches(trimmed, dep)
    })
}

/// Test-only entry point that skips project root detection.
#[cfg(test)]
pub fn run_test_init(root: &Path) -> Result<()> {
    run_in(root)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cargo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Rust/Cargo");
        assert!(stacks[0].facts.len() >= 3);
    }

    #[test]
    fn test_detect_cargo_with_clippy() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("clippy.toml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert!(stacks[0].facts.iter().any(|f| f.label == "clippy passes"));
    }

    #[test]
    fn test_detect_node() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Node.js");
    }

    #[test]
    fn test_detect_node_with_scripts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"test":"jest","lint":"eslint .","build":"tsc"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Node.js");
        assert!(stacks[0].facts.iter().any(|f| f.label == "all tests pass"));
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code passes linting")
        );
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "project builds successfully")
        );
    }

    #[test]
    fn test_detect_node_dep_fallback_test() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"vitest":"1.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let test_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "all tests pass")
            .unwrap();
        assert_eq!(test_fact.command.as_deref(), Some("npx vitest run"));
    }

    #[test]
    fn test_detect_node_dep_fallback_lint() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"eslint":"9.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let lint_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "code passes linting")
            .unwrap();
        assert_eq!(lint_fact.command.as_deref(), Some("npx eslint ."));
    }

    #[test]
    fn test_detect_node_dep_fallback_typescript() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"typescript":"5.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let tc_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "type checking passes")
            .unwrap();
        assert_eq!(tc_fact.command.as_deref(), Some("npx tsc --noEmit"));
    }

    #[test]
    fn test_detect_node_dep_fallback_prettier() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"prettier":"3.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let fmt_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "code is formatted")
            .unwrap();
        assert_eq!(fmt_fact.command.as_deref(), Some("npx prettier --check ."));
    }

    #[test]
    fn test_detect_node_dep_fallback_biome() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"@biomejs/biome":"1.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let lint_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "code passes linting")
            .unwrap();
        assert_eq!(lint_fact.command.as_deref(), Some("npx biome check"));
    }

    #[test]
    fn test_detect_node_script_takes_priority_over_dep() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"test":"vitest"},"devDependencies":{"jest":"29.0.0"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        let test_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "all tests pass")
            .unwrap();
        // Uses the script (npm test), not the dep fallback (npx jest)
        assert_eq!(test_fact.command.as_deref(), Some("npm test"));
    }

    #[test]
    fn test_detect_node_yarn() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"test":"jest"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        let test_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "all tests pass")
            .unwrap();
        assert_eq!(test_fact.command.as_deref(), Some("yarn test"));
    }

    #[test]
    fn test_detect_node_pnpm() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"scripts":{"lint":"eslint"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        let lint_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "code passes linting")
            .unwrap();
        assert_eq!(lint_fact.command.as_deref(), Some("pnpm run lint"));
    }

    #[test]
    fn test_detect_node_pnpm_dep_fallback() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"devDependencies":{"eslint":"9.0.0"}}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        let lint_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "code passes linting")
            .unwrap();
        assert_eq!(lint_fact.command.as_deref(), Some("pnpm exec eslint ."));
    }

    #[test]
    fn test_detect_nextjs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{"dependencies":{"next":"14.0.0"},"scripts":{"build":"next build"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks[0].name, "Next.js");
    }

    #[test]
    fn test_detect_deno() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("deno.json"),
            r#"{"tasks":{"test":"deno test --allow-all"}}"#,
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Deno");
    }

    #[test]
    fn test_deno_takes_priority_over_node() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("deno.json"), "{}").unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Deno");
    }

    #[test]
    fn test_detect_python_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Python (pyproject.toml)");
    }

    #[test]
    fn test_detect_python_pyproject_with_tools() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n\n[tool.pytest.ini_options]\n\n[tool.ruff]\n\n[tool.mypy]\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert!(stacks[0].facts.iter().any(|f| f.label == "all tests pass"));
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code passes linting")
        );
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "type checking passes")
        );
    }

    #[test]
    fn test_detect_python_pyproject_dep_fallback() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\ndependencies = [\n  \"flask\",\n]\n\n[dependency-groups]\ndev = [\n  \"pytest>=7.0\",\n  \"ruff\",\n  \"mypy\",\n]\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert!(stacks[0].facts.iter().any(|f| f.label == "all tests pass"));
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code passes linting")
        );
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "type checking passes")
        );
    }

    #[test]
    fn test_detect_python_poetry() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.poetry]\nname = \"test\"\n\n[tool.pytest.ini_options]\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("poetry.lock"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        let test_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "all tests pass")
            .unwrap();
        assert!(
            test_fact
                .command
                .as_deref()
                .unwrap()
                .starts_with("poetry run")
        );
    }

    #[test]
    fn test_detect_python_uv() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"test\"\n\n[tool.pytest.ini_options]\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("uv.lock"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        let test_fact = stacks[0]
            .facts
            .iter()
            .find(|f| f.label == "all tests pass")
            .unwrap();
        assert!(test_fact.command.as_deref().unwrap().starts_with("uv run"));
    }

    #[test]
    fn test_detect_python_requirements() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Python (requirements.txt)");
    }

    #[test]
    fn test_detect_python_requirements_with_deps() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("requirements.txt"),
            "flask\npytest>=7.0\nruff\nblack==24.0\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert!(stacks[0].facts.iter().any(|f| f.label == "all tests pass"));
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code passes linting")
        );
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code is formatted")
        );
    }

    #[test]
    fn test_detect_python_prefers_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]\n").unwrap();
        std::fs::write(dir.path().join("requirements.txt"), "flask\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Python (pyproject.toml)");
    }

    #[test]
    fn test_detect_go() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example.com/test\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Go");
    }

    #[test]
    fn test_detect_go_with_golangci() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("go.mod"), "module example.com/test\n").unwrap();
        std::fs::write(dir.path().join(".golangci.yml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert!(stacks[0].facts.iter().any(|f| f.label == "linter passes"));
    }

    #[test]
    fn test_detect_ruby() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Ruby");
    }

    #[test]
    fn test_detect_ruby_with_rspec_and_rubocop() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Gemfile"), "").unwrap();
        std::fs::create_dir(dir.path().join("spec")).unwrap();
        std::fs::write(dir.path().join(".rubocop.yml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert!(
            stacks[0].facts.iter().any(|f| f.label == "all tests pass"
                && f.command.as_deref() == Some("bundle exec rspec"))
        );
        assert!(
            stacks[0]
                .facts
                .iter()
                .any(|f| f.label == "code passes linting")
        );
    }

    #[test]
    fn test_detect_java_maven() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pom.xml"), "<project></project>").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Java (Maven)");
    }

    #[test]
    fn test_detect_java_gradle() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Java (Gradle)");
    }

    #[test]
    fn test_detect_java_gradle_kts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.gradle.kts"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Java (Gradle)");
    }

    #[test]
    fn test_detect_zig() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("build.zig"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Zig");
    }

    #[test]
    fn test_detect_gleam() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("gleam.toml"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Gleam");
    }

    #[test]
    fn test_detect_dart() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pubspec.yaml"), "name: test\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Dart");
    }

    #[test]
    fn test_detect_flutter() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pubspec.yaml"),
            "name: test\nflutter:\n  sdk: flutter\n",
        )
        .unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Flutter");
    }

    #[test]
    fn test_detect_docker() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Dockerfile"), "FROM alpine\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Docker");
    }

    #[test]
    fn test_detect_terraform() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tf"), "").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Terraform");
    }

    #[test]
    fn test_detect_multiple_stacks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 2);
    }

    #[test]
    fn test_detect_no_stacks() {
        let dir = tempfile::tempdir().unwrap();
        let stacks = detect_stacks(dir.path());
        assert!(stacks.is_empty());
    }

    #[test]
    fn test_generate_content_no_stacks() {
        let content = generate_facts_content(&[]);
        assert!(content.contains("# project"));
        assert!(content.contains("- project is set up"));
    }

    #[test]
    fn test_generate_content_with_stack() {
        let stacks = vec![DetectedStack {
            name: "Rust/Cargo",
            marker: "Cargo.toml",
            facts: vec![
                StackFact {
                    label: "project uses Rust with Cargo".to_string(),
                    command: Some("test -f Cargo.toml".to_string()),
                },
                StackFact {
                    label: "all tests pass".to_string(),
                    command: Some("cargo test --quiet".to_string()),
                },
            ],
        }];
        let content = generate_facts_content(&stacks);
        assert!(content.contains("# project"));
        assert!(content.contains("- label: project uses Rust with Cargo"));
        assert!(content.contains("  command: test -f Cargo.toml"));
        assert!(content.contains("- label: all tests pass"));
        assert!(content.contains("  command: cargo test --quiet"));
    }

    #[test]
    fn test_init_errors_when_facts_is_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::create_dir(dir.path().join(".facts")).unwrap();

        let result = run_in(dir.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("not a file"), "unexpected error: {msg}");
    }

    #[test]
    fn test_init_skips_existing_facts_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".facts"), "- existing fact\n").unwrap();

        let result = run_in(dir.path());
        assert!(result.is_ok());

        let content = std::fs::read_to_string(dir.path().join(".facts")).unwrap();
        assert_eq!(content, "- existing fact\n");

        // Skills still installed
        assert!(dir.path().join(".agents/skills/facts/SKILL.md").exists());
    }

    #[test]
    fn test_init_creates_file_and_skills() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();

        let result = run_in(dir.path());
        assert!(result.is_ok(), "init failed: {}", result.unwrap_err());

        assert!(dir.path().join(".facts").exists());
        let content = std::fs::read_to_string(dir.path().join(".facts")).unwrap();
        assert!(content.contains("cargo"));

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
    fn test_init_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();

        assert!(run_in(dir.path()).is_ok());
        let content_first = std::fs::read_to_string(dir.path().join(".facts")).unwrap();

        assert!(run_in(dir.path()).is_ok());
        let content_second = std::fs::read_to_string(dir.path().join(".facts")).unwrap();
        assert_eq!(content_first, content_second);
    }

    #[test]
    fn test_install_skill_creates() {
        let dir = tempfile::tempdir().unwrap();
        install_skill(dir.path(), "test-skill", "# skill content").unwrap();

        let path = dir.path().join(".agents/skills/test-skill/SKILL.md");
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# skill content");
    }

    #[test]
    fn test_install_skill_updates() {
        let dir = tempfile::tempdir().unwrap();
        install_skill(dir.path(), "test-skill", "v1").unwrap();
        install_skill(dir.path(), "test-skill", "v2").unwrap();

        let path = dir.path().join(".agents/skills/test-skill/SKILL.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "v2");
    }

    #[test]
    fn test_install_skill_skips_identical() {
        let dir = tempfile::tempdir().unwrap();
        install_skill(dir.path(), "test-skill", "same").unwrap();
        install_skill(dir.path(), "test-skill", "same").unwrap();

        let path = dir.path().join(".agents/skills/test-skill/SKILL.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "same");
    }

    #[cfg(unix)]
    #[test]
    fn test_link_skill_for_claude() {
        let dir = tempfile::tempdir().unwrap();
        install_skill(dir.path(), "test-skill", "content").unwrap();
        link_skill_for_claude(dir.path(), "test-skill").unwrap();

        let link = dir.path().join(".claude/skills/test-skill");
        assert!(link.is_symlink());
        // The symlink target dir should contain SKILL.md
        assert!(link.join("SKILL.md").exists());
        assert_eq!(
            std::fs::read_to_string(link.join("SKILL.md")).unwrap(),
            "content"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_link_skill_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        install_skill(dir.path(), "test-skill", "content").unwrap();
        link_skill_for_claude(dir.path(), "test-skill").unwrap();
        // Second call should succeed without error.
        link_skill_for_claude(dir.path(), "test-skill").unwrap();

        let link = dir.path().join(".claude/skills/test-skill");
        assert!(link.is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn test_link_skill_preserves_real_dir() {
        let dir = tempfile::tempdir().unwrap();
        // Create a real directory at the link location.
        let real_dir = dir.path().join(".claude/skills/test-skill");
        std::fs::create_dir_all(&real_dir).unwrap();
        std::fs::write(real_dir.join("SKILL.md"), "user content").unwrap();

        install_skill(dir.path(), "test-skill", "agent content").unwrap();
        link_skill_for_claude(dir.path(), "test-skill").unwrap();

        // Real dir should be untouched.
        assert!(!real_dir.is_symlink());
        assert_eq!(
            std::fs::read_to_string(real_dir.join("SKILL.md")).unwrap(),
            "user content"
        );
    }

    // -- Dependency helpers --

    #[test]
    fn test_dep_line_matches() {
        assert!(dep_line_matches("pytest", "pytest"));
        assert!(dep_line_matches("pytest>=7.0", "pytest"));
        assert!(dep_line_matches("pytest==7.0", "pytest"));
        assert!(dep_line_matches("pytest[extra]", "pytest"));
        assert!(dep_line_matches("pytest ~=7.0", "pytest"));
        assert!(dep_line_matches("pytest >=7.0", "pytest"));
        assert!(!dep_line_matches("pytest-cov", "pytest"));
        assert!(!dep_line_matches("pypytest", "pytest"));
    }

    #[test]
    fn test_dep_in_pyproject() {
        let content = "[project]\ndependencies = [\n  \"flask>=2.0\",\n  \"sqlalchemy\",\n]\n\n[dependency-groups]\ndev = [\n  \"pytest>=7.0\",\n  \"ruff\",\n]\n";
        assert!(dep_in_pyproject(content, "flask"));
        assert!(dep_in_pyproject(content, "pytest"));
        assert!(dep_in_pyproject(content, "ruff"));
        assert!(!dep_in_pyproject(content, "django"));
    }

    #[test]
    fn test_dep_in_requirements() {
        let content = "flask>=2.0\npytest\n# comment\nruff==0.1.0\n";
        assert!(dep_in_requirements(content, "flask"));
        assert!(dep_in_requirements(content, "pytest"));
        assert!(dep_in_requirements(content, "ruff"));
        assert!(!dep_in_requirements(content, "django"));
    }

    // -- JSON / TOML helpers --

    #[test]
    fn test_json_section_has_key() {
        let json = r#"{"scripts":{"test":"jest","lint":"eslint ."}}"#;
        assert!(json_section_has_key(json, "scripts", "test"));
        assert!(json_section_has_key(json, "scripts", "lint"));
        assert!(!json_section_has_key(json, "scripts", "build"));
    }

    #[test]
    fn test_json_section_has_key_nested() {
        let json = r#"{"a":{"scripts":"value"},"scripts":{"test":"jest"}}"#;
        assert!(json_section_has_key(json, "scripts", "test"));
    }

    #[test]
    fn test_json_section_has_key_string_value() {
        let json = r#"{"name":"scripts","other":{"test":"x"}}"#;
        assert!(!json_section_has_key(json, "scripts", "test"));
    }

    #[test]
    fn test_json_has_dep() {
        let json = r#"{"dependencies":{"next":"14.0.0"},"devDependencies":{"typescript":"5"}}"#;
        assert!(json_has_dep(json, "next"));
        assert!(json_has_dep(json, "typescript"));
        assert!(!json_has_dep(json, "react"));
    }

    #[test]
    fn test_toml_has_section() {
        let toml = "[project]\nname = \"test\"\n\n[tool.pytest.ini_options]\n\n[tool.ruff]\n";
        assert!(toml_has_section(toml, "project"));
        assert!(toml_has_section(toml, "tool.pytest"));
        assert!(toml_has_section(toml, "tool.ruff"));
        assert!(!toml_has_section(toml, "tool.mypy"));
    }

    // -- Agent docs --

    #[test]
    fn test_agent_docs_creates_agents_md_when_no_md_exists() {
        let dir = tempfile::tempdir().unwrap();
        install_agent_docs(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert!(content.contains(SECTION_START));
        assert!(content.contains(SECTION_END));
        assert!(content.contains("facts check"));
        assert!(!dir.path().join("CLAUDE.md").exists());
    }

    #[test]
    fn test_agent_docs_appends_to_existing_claude_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            "# My Project\n\nExisting content.\n",
        )
        .unwrap();
        install_agent_docs(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(content.starts_with("# My Project\n\nExisting content.\n"));
        assert!(content.contains(SECTION_START));
        assert!(!dir.path().join("AGENTS.md").exists());
    }

    #[test]
    fn test_agent_docs_appends_to_existing_agents_md() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Agents\n").unwrap();
        install_agent_docs(dir.path()).unwrap();
        let content = std::fs::read_to_string(dir.path().join("AGENTS.md")).unwrap();
        assert!(content.starts_with("# Agents\n"));
        assert!(content.contains(SECTION_START));
    }

    #[test]
    fn test_agent_docs_writes_to_both_when_both_exist() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# Agents\n").unwrap();
        install_agent_docs(dir.path()).unwrap();
        assert!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md"))
                .unwrap()
                .contains(SECTION_START)
        );
        assert!(
            std::fs::read_to_string(dir.path().join("AGENTS.md"))
                .unwrap()
                .contains(SECTION_START)
        );
    }

    #[test]
    fn test_agent_docs_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        install_agent_docs(dir.path()).unwrap();
        let first = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        install_agent_docs(dir.path()).unwrap();
        let second = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn test_agent_docs_replaces_stale_section() {
        let dir = tempfile::tempdir().unwrap();
        let stale =
            "# Project\n\n<!-- facts:start -->\nold content\n<!-- facts:end -->\n\n## Other\n";
        std::fs::write(dir.path().join("CLAUDE.md"), stale).unwrap();
        install_agent_docs(dir.path()).unwrap();
        let result = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(result.contains(AGENT_DOCS_SECTION));
        assert!(!result.contains("old content"));
        assert!(result.contains("# Project"));
        assert!(result.contains("## Other"));
    }

    #[test]
    fn test_remove_agent_docs_from_middle() {
        let dir = tempfile::tempdir().unwrap();
        let content =
            format!("# Top\n\nSome text.\n\n{AGENT_DOCS_SECTION}\n\n## Bottom\n\nMore text.\n");
        std::fs::write(dir.path().join("CLAUDE.md"), &content).unwrap();
        remove_agent_docs(dir.path()).unwrap();
        let result = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert!(!result.contains(SECTION_START));
        assert!(result.contains("# Top"));
        assert!(result.contains("## Bottom"));
    }

    #[test]
    fn test_remove_agent_docs_only_section_deletes_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            format!("{AGENT_DOCS_SECTION}\n"),
        )
        .unwrap();
        remove_agent_docs(dir.path()).unwrap();
        assert!(!dir.path().join("AGENTS.md").exists());
    }

    #[test]
    fn test_remove_agent_docs_from_both_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("CLAUDE.md"),
            format!("# Claude\n\n{AGENT_DOCS_SECTION}\n"),
        )
        .unwrap();
        std::fs::write(
            dir.path().join("AGENTS.md"),
            format!("{AGENT_DOCS_SECTION}\n"),
        )
        .unwrap();
        remove_agent_docs(dir.path()).unwrap();
        let claude = std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
        assert_eq!(claude, "# Claude\n");
        assert!(!dir.path().join("AGENTS.md").exists());
    }

    #[test]
    fn test_remove_agent_docs_noop_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "# Claude\n").unwrap();
        remove_agent_docs(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
            "# Claude\n"
        );
    }
}
