/// The `init` subcommand — scaffold a .facts file with detected stack.
///
/// Detects well-known framework/runtime combos by checking for marker files
/// in the project root, then generates initial facts for each detected stack.

use anyhow::Result;
use std::path::Path;

use crate::project;

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

/// All known stack detectors.
static DETECTORS: &[(&str, &str, &[(&str, Option<&str>)])] = &[
    (
        "Rust/Cargo",
        "Cargo.toml",
        &[
            ("project uses Rust with Cargo", Some("test -f Cargo.toml")),
            ("project compiles successfully", Some("cargo check")),
            ("all tests pass", Some("cargo test --quiet")),
            ("code is formatted", Some("cargo fmt --check")),
        ],
    ),
    (
        "Node.js",
        "package.json",
        &[
            ("project uses Node.js", Some("test -f package.json")),
            (
                "dependencies are installed",
                Some("test -d node_modules"),
            ),
            (
                "project has a start script",
                Some("node -e \"require('./package.json').scripts.start\""),
            ),
        ],
    ),
    (
        "Python (pyproject.toml)",
        "pyproject.toml",
        &[
            ("project uses Python", Some("test -f pyproject.toml")),
            (
                "project has a build system defined",
                Some("python3 -c \"import tomllib; t=tomllib.load(open('pyproject.toml','rb')); assert 'build-system' in t\""),
            ),
            ("all tests pass", Some("python3 -m pytest --quiet")),
        ],
    ),
    (
        "Python (requirements.txt)",
        "requirements.txt",
        &[
            (
                "project uses Python with requirements.txt",
                Some("test -f requirements.txt"),
            ),
            (
                "dependencies are installed",
                Some("pip3 check"),
            ),
            ("all tests pass", Some("python3 -m pytest --quiet")),
        ],
    ),
    (
        "Go",
        "go.mod",
        &[
            ("project uses Go modules", Some("test -f go.mod")),
            ("project builds successfully", Some("go build ./...")),
            ("all tests pass", Some("go test ./...")),
            ("code is formatted", Some("gofmt -l . | grep -c . | grep -q ^0$")),
        ],
    ),
    (
        "Ruby",
        "Gemfile",
        &[
            ("project uses Ruby with Bundler", Some("test -f Gemfile")),
            (
                "dependencies are installed",
                Some("bundle check"),
            ),
            ("all tests pass", Some("bundle exec rake test")),
        ],
    ),
    (
        "Java (Maven)",
        "pom.xml",
        &[
            ("project uses Java with Maven", Some("test -f pom.xml")),
            ("project compiles successfully", Some("mvn compile -q")),
            ("all tests pass", Some("mvn test -q")),
        ],
    ),
    (
        "Java (Gradle)",
        "build.gradle",
        &[
            ("project uses Java with Gradle", Some("test -f build.gradle")),
            (
                "project compiles successfully",
                Some("./gradlew compileJava --quiet"),
            ),
            ("all tests pass", Some("./gradlew test --quiet")),
        ],
    ),
    (
        "Elixir",
        "mix.exs",
        &[
            ("project uses Elixir with Mix", Some("test -f mix.exs")),
            ("project compiles successfully", Some("mix compile --warnings-as-errors")),
            ("all tests pass", Some("mix test")),
        ],
    ),
    (
        "PHP (Composer)",
        "composer.json",
        &[
            ("project uses PHP with Composer", Some("test -f composer.json")),
            ("dependencies are installed", Some("test -d vendor")),
            ("all tests pass", Some("./vendor/bin/phpunit")),
        ],
    ),
    (
        "Swift",
        "Package.swift",
        &[
            ("project uses Swift Package Manager", Some("test -f Package.swift")),
            ("project builds successfully", Some("swift build")),
            ("all tests pass", Some("swift test")),
        ],
    ),
    (
        "C# (.NET)",
        "*.csproj",
        &[
            ("project uses .NET", Some("ls *.csproj >/dev/null 2>&1")),
            ("project builds successfully", Some("dotnet build --nologo -q")),
            ("all tests pass", Some("dotnet test --nologo -q")),
        ],
    ),
];

/// Run the init subcommand (auto-detects project root).
pub fn run() -> Result<()> {
    let root = project::find_project_root()?;
    run_in(&root)
}

/// Run the init subcommand in a given root directory.
fn run_in(root: &Path) -> Result<()> {
    let facts_path = root.join(".facts");

    if facts_path.exists() {
        anyhow::bail!(".facts already exists in {}", root.display());
    }

    let stacks = detect_stacks(root);
    let content = generate_facts_content(&stacks);
    std::fs::write(&facts_path, &content)?;

    println!("created {}", facts_path.display());
    if stacks.is_empty() {
        println!("no known frameworks detected, scaffolded a minimal .facts file");
    } else {
        let names: Vec<&str> = stacks.iter().map(|s| s.name).collect();
        println!("detected: {}", names.join(", "));
    }

    Ok(())
}

/// Detect project stacks by checking for marker files.
pub fn detect_stacks(root: &Path) -> Vec<DetectedStack> {
    let mut stacks = Vec::new();

    for (name, marker, facts) in DETECTORS {
        // Handle glob-style markers like "*.csproj"
        let found = if marker.contains('*') {
            // Simple glob: check if any matching file exists
            let pattern = marker.replace('*', "");
            std::fs::read_dir(root)
                .ok()
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .any(|e| {
                            e.file_name()
                                .to_str()
                                .is_some_and(|n| n.ends_with(&pattern))
                        })
                })
                .unwrap_or(false)
        } else {
            root.join(marker).exists()
        };

        if found {
            // Skip "Python (requirements.txt)" if pyproject.toml was already detected
            if *marker == "requirements.txt"
                && stacks.iter().any(|s: &DetectedStack| s.marker == "pyproject.toml")
            {
                continue;
            }

            stacks.push(DetectedStack {
                name,
                marker,
                facts: facts
                    .iter()
                    .map(|(label, cmd)| StackFact {
                        label: label.to_string(),
                        command: cmd.map(|c| c.to_string()),
                    })
                    .collect(),
            });
        }
    }

    stacks
}

/// Generate .facts file content from detected stacks.
pub fn generate_facts_content(stacks: &[DetectedStack]) -> String {
    let mut out = String::new();

    if stacks.is_empty() {
        // Minimal scaffold
        out.push_str("# project\n\n");
        out.push_str("- project is set up and ready for development\n");
        return out;
    }

    // Project header from the first detected stack
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_cargo() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Rust/Cargo");
        assert!(stacks[0].facts.len() >= 3);
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
    fn test_detect_python_pyproject() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pyproject.toml"), "[project]\nname = \"test\"\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Python (pyproject.toml)");
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
    fn test_detect_ruby() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Gemfile"), "source 'https://rubygems.org'\n").unwrap();

        let stacks = detect_stacks(dir.path());
        assert_eq!(stacks.len(), 1);
        assert_eq!(stacks[0].name, "Ruby");
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
    fn test_init_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".facts"), "- existing fact\n").unwrap();

        let result = run_in(dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("already exists"),
            "expected 'already exists' error, got: {err}"
        );

        // Verify original content is unchanged
        let content = std::fs::read_to_string(dir.path().join(".facts")).unwrap();
        assert_eq!(content, "- existing fact\n");
    }

    #[test]
    fn test_init_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"t\"\n").unwrap();

        let result = run_in(dir.path());

        assert!(result.is_ok(), "init failed: {}", result.unwrap_err());
        assert!(dir.path().join(".facts").exists());

        let content = std::fs::read_to_string(dir.path().join(".facts")).unwrap();
        assert!(content.contains("Cargo"));
    }
}
