---
name: facts-discover
description: >
  Scan the codebase and sync .facts files to match reality — add missing
  facts, fix inaccurate ones, remove obsolete ones. Use when asked to
  discover facts, bootstrap or update a fact sheet, scan the codebase for
  truths, sync facts to match the code, or audit the fact sheet for accuracy.
---

# facts-discover

You are a fact sheet maintainer. Your job is to scan the codebase and make the `.facts` file(s) an accurate, complete mirror of the project — in a single session.

## Goal

Every fact must reflect something that is actually true about the codebase right now. Remove lies, add truths, fix inaccuracies. **Maximize the number of command-validated facts.** A fact with a passing command is worth far more than a manual one — it is self-enforcing and catches regressions automatically. When you are done, the fact sheet should be a reliable, largely automated specification that another agent can implement against or validate from.

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

### 3b. Add commands to manual facts

This is a critical step. Go through every manual fact and ask: **can this be checked with a short shell command?** If yes, add one with `facts edit <id> --command "..."`. The goal is to convert as many manual facts as possible into automated ones.

Good check commands are:
- **Fast** — runs in under a second (grep, test, jq, wc, head)
- **Idempotent** — read-only, no side effects
- **Stable** — does not break on unrelated changes (avoid line-count checks, match patterns not positions)
- **Silent on success** — exit 0 means the fact holds, non-zero means it doesn't

Common patterns:

```sh
# Dependency or config key exists
grep -q '^some_dep' Cargo.toml
grep -q '"express"' package.json

# File or directory exists
test -f tests/cli.rs
test -d src/components

# A binary or tool is available
command -v facts >/dev/null

# Build or test suite passes
cargo build --quiet 2>/dev/null
npm test --silent

# A pattern appears (or does not appear) in source
grep -rq 'use async' src/
! grep -rq 'unsafe' src/

# Count-based checks (use ranges, not exact numbers)
test $(find src -name '*.rs' | wc -l) -ge 10

# Structural checks via CLI tools
jq -e '.scripts.test' package.json >/dev/null
```

Not every fact can or should have a command. Skip facts that are:
- Subjective or qualitative ("extreme simplicity", "codebase is DRY")
- About human processes ("bump version, commit, push")
- About external systems you can't query locally
- So complex to check that the command itself becomes a maintenance burden

### 4. Add missing facts

Identify important truths not yet in the fact sheet. Add them with `facts add`:

```
facts add "the project uses PostgreSQL for persistence" --section architecture
facts add "project builds successfully" --command "cargo build" --section ci
```

Prefer facts that are:
- **Atomic** — one truth per fact
- **Automated** — include a check command whenever possible; a fact without a command is a fact that can silently go stale
- **Stable** — unlikely to change with every commit
- **Useful** — helps an agent understand or validate the project

Do not add trivially obvious facts ("the project has files") or volatile ones ("there are 47 tests").

### 5. Organize

Group related facts into sections using `--section`. Section paths support nesting (e.g. `api/auth`, `cli/subcommands`). Keep sections focused — split broad ones.

### 6. Validate and report

Run `facts check` to confirm all command-facts pass (this also lints the files).

Report what changed: facts added, edited, removed, commands added. Include a coverage summary: how many facts are now command-validated vs manual. If any areas of the codebase were ambiguous or couldn't be fully captured, say so.

## Guidelines

- Keep fact labels concise and declarative.
- **Command coverage is a key metric.** Every manual fact is a fact that can silently become false. Treat adding commands to existing manual facts as equally important to adding new facts.
- When writing check commands, prefer `grep -q`, `test -f`, `test -d`, `jq -e`, and similar fast read-only checks. Avoid commands that build, install, or modify anything unless that is the point of the fact (e.g. "project builds successfully").
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
facts add "API rate limits to 100 req/min per key" --section api/limits \
  --command "grep -q 'rate_limit.*100' src/middleware.rs"

# An old fact about Python is no longer true — project migrated to Rust
facts remove p2q

# Add commands to manual facts that can be automated
facts edit a3b --command "test -f docker-compose.yml"
facts edit c7d --command "grep -q 'serde' Cargo.toml"
facts edit f2g --command "test -d src/api && test -d src/core"

# Verify everything
facts check

# Report: 3 added, 1 edited, 1 removed, 3 commands added
# Coverage: 12/20 facts automated (was 8/17)
```
