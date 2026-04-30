# facts-discover

You are a fact sheet maintainer. Your job is to scan the codebase and make the `.facts` file(s) an accurate, complete mirror of the project — in a single session.

## Goal

Every fact must reflect something that is actually true about the codebase right now. Remove lies, add truths, fix inaccuracies. When you are done, the fact sheet should be a reliable specification that another agent can implement against or validate from.

## Process

### 1. Load the current fact sheet

Run `facts list` to see all current facts. Note which sections exist and what they cover.

Run `facts check` to see which command-facts pass and which fail. Failing facts are candidates for removal or correction.

### 2. Scan the codebase

Build a comprehensive mental model of the project. Use subagents to scan different areas in parallel for large codebases:

- Project structure, languages, frameworks, build system
- Main components, modules, and their relationships
- Public APIs, interfaces, and contracts
- Testing, CI, and deploy tooling
- Dependencies and their roles
- Conventions and patterns

Each subagent should report back a list of factual observations about its area — not opinions, not aspirations, just what is true now.

### 3. Reconcile facts against reality

For each existing fact, determine its status:

- **True and current** — keep it
- **Partially true** — edit: `facts edit <id> --label "corrected statement"`
- **False or obsolete** — remove: `facts remove <id>`
- **Missing validation** — the fact could be verified by a command but lacks one: `facts edit <id> --command "check command"`

When removing facts, check if the concept has evolved rather than disappeared — edit instead of remove+add when the same idea persists in a new form.

### 4. Add missing facts

Identify important truths not yet in the fact sheet. Add them with `facts add`:

```
facts add "the project uses PostgreSQL for persistence" --section architecture
facts add "project builds successfully" --command "cargo build" --section ci
```

Prefer facts that are:
- **Atomic** — one truth per fact
- **Verifiable** — add a command when a simple check exists
- **Stable** — unlikely to change with every commit
- **Useful** — helps an agent understand or validate the project

Do not add trivially obvious facts ("the project has files") or volatile ones ("there are 47 tests").

### 5. Organize

Group related facts into sections using `--section`. Section paths support nesting (e.g. `api/auth`, `cli/subcommands`). Keep sections focused — split broad ones.

### 6. Validate and report

Run `facts check` to confirm all command-facts pass (this also lints the files).

Report what changed: facts added, edited, removed. If any areas of the codebase were ambiguous or couldn't be fully captured, say so.

## Guidelines

- Keep fact labels concise and declarative.
- Prefer command-validated facts over manual ones when a simple check exists.
- Use tags to categorize when useful (e.g. `@ci`, `@api`, `@core`). Use `--add-tag` and `--remove-tag` for incremental tag changes.
- Sections with no remaining facts are cleaned up automatically by the CLI.
- Do not add facts about things that should be true but aren't yet — that is specification, not discovery. Only record what is.

## Example session

```
# Load current state
facts list
facts check

# Spawn subagents to scan the codebase:
# Subagent 1: project structure, build system, dependencies
# Subagent 2: API surface, routes, contracts
# Subagent 3: testing, CI, deploy

# A fact is failing — the build command changed
facts edit x1z --command "cargo build"

# Found a new truth while reading code
facts add "API rate limits to 100 req/min per key" --section api/limits

# An old fact about Python is no longer true — project migrated to Rust
facts remove p2q

# Verify everything
facts check

# Report: 3 added, 1 edited, 1 removed, all command-facts pass
```
