mod id;
mod list;
mod model;
mod parser;
mod project;
mod tags;

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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let opts = match cli.command {
        Some(Command::List {
            file,
            section,
            has_command,
            manual,
            tags,
        }) => list::ListOptions {
            file_filter: file,
            section_filter: section,
            has_command,
            manual,
            tags_expr: tags,
        },
        None => list::ListOptions {
            file_filter: None,
            section_filter: None,
            has_command: false,
            manual: false,
            tags_expr: None,
        },
    };
    list::run(&opts)?;

    Ok(())
}
