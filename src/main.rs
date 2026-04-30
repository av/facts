mod add;
mod check;
mod color;
mod edit;
mod id;
mod init;
mod lint;
mod list;
mod model;
mod parser;
mod project;
mod remove;
mod tags;
mod writer;

use clap::{Parser, Subcommand};

/// A CLI for fact-driven development with coding agents.
#[derive(Parser)]
#[command(name = "facts", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show facts in file order (default when no subcommand given).
    List {
        /// Filter by file name.
        #[arg(long)]
        file: Option<String>,

        /// Filter by section path.
        #[arg(long)]
        section: Option<String>,

        /// Show only facts with a validation command.
        #[arg(long)]
        has_command: bool,

        /// Show only manual facts (no validation command).
        #[arg(long)]
        manual: bool,

        /// Boolean tag filter expression (e.g. "mvp and not blocked").
        #[arg(long)]
        tags: Option<String>,
    },

    /// Run all command-facts, report pass/fail/manual.
    Check {
        /// Boolean tag filter expression (e.g. "mvp and not blocked").
        #[arg(long)]
        tags: Option<String>,

        /// Per-command timeout in seconds.
        #[arg(long)]
        timeout: Option<u64>,
    },

    /// Append a fact to a file and section.
    Add {
        /// The fact label text.
        label: String,

        /// Target section path (e.g. "cli/add"). Created if needed.
        #[arg(long)]
        section: Option<String>,

        /// Target .facts file (default: ".facts"). Created if needed.
        #[arg(long)]
        file: Option<String>,

        /// Validation command.
        #[arg(long)]
        command: Option<String>,

        /// Explicit ID override.
        #[arg(long)]
        id: Option<String>,

        /// Comma-separated tags (e.g. "mvp,core").
        #[arg(long)]
        tags: Option<String>,
    },

    /// Remove a fact by ID, outputs what was removed.
    Remove {
        /// The ID of the fact to remove.
        id: String,
    },

    /// Modify a fact by ID.
    Edit {
        /// The ID of the fact to edit.
        id: String,

        /// New label text.
        #[arg(long)]
        label: Option<String>,

        /// New validation command.
        #[arg(long)]
        command: Option<String>,

        /// New explicit ID.
        #[arg(long, name = "new-id")]
        new_id: Option<String>,

        /// New tags (comma-separated, e.g. "mvp,core").
        #[arg(long)]
        tags: Option<String>,
    },

    /// Validate that fact sheets are parseable.
    Lint {
        /// Lint a specific file instead of all *.facts files.
        #[arg(long)]
        file: Option<String>,
    },

    /// Scaffold a .facts file with detected project stack.
    Init,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::List {
            file,
            section,
            has_command,
            manual,
            tags,
        }) => {
            let opts = list::ListOptions {
                file_filter: file,
                section_filter: section,
                has_command,
                manual,
                tags_expr: tags,
            };
            list::run(&opts)?;
        }
        Some(Command::Check { tags, timeout }) => {
            let opts = check::CheckOptions {
                tags_expr: tags,
                timeout,
            };
            let all_passed = check::run(&opts)?;
            if !all_passed {
                std::process::exit(1);
            }
        }
        Some(Command::Add {
            label,
            section,
            file,
            command,
            id,
            tags,
        }) => {
            let tags = tags
                .map(|t| add::parse_tags(&t))
                .unwrap_or_default();
            let opts = add::AddOptions {
                label,
                file,
                section,
                command,
                id,
                tags,
            };
            add::run(&opts)?;
        }
        Some(Command::Remove { id }) => {
            remove::run(&id)?;
        }
        Some(Command::Edit {
            id,
            label,
            command,
            new_id,
            tags,
        }) => {
            let tags = tags.map(|t| add::parse_tags(&t));
            let opts = edit::EditOptions {
                target_id: id,
                label,
                command,
                new_id,
                tags,
            };
            edit::run(&opts)?;
        }
        Some(Command::Lint { file }) => {
            let opts = lint::LintOptions { file };
            let all_passed = lint::run(&opts)?;
            if !all_passed {
                std::process::exit(1);
            }
        }
        Some(Command::Init) => {
            init::run()?;
        }
        None => {
            let opts = list::ListOptions {
                file_filter: None,
                section_filter: None,
                has_command: false,
                manual: false,
                tags_expr: None,
            };
            list::run(&opts)?;
        }
    }

    Ok(())
}
