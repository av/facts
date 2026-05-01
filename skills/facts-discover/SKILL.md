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

Every fact must reflect something that is actually true about the codebase right now. Remove lies, add truths, fix inaccuracies. When you are done, the fact sheet should be a reliable specification that another agent can implement against or validate from.

Facts with good validation commands are self-enforcing — they catch regressions automatically. But **a manual fact is better than a fact with a useless command.** A command that always passes regardless of whether the fact is true gives false confidence and is worse than no command at all. Only add a command when it genuinely tests the claim.

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

Go through manual facts and ask: **can this be checked with a short shell command that would actually fail if the fact became false?**

That second part is the hard filter. Before adding a command, apply this test:

> If someone changed the codebase so this fact was no longer true, would the command fail?

If the answer is no — or only maybe — leave the fact manual.

#### What makes a command meaningful

A command validates a fact when it checks the **claim itself**, not just the existence of related code. The command should be:

- **Falsifiable** — would actually break if the fact became untrue
- **Fast** — runs in under a second (grep, test, jq, wc, head)
- **Idempotent** — read-only, no side effects
- **Stable** — does not break on unrelated changes (avoid line-count checks, match patterns not positions)
- **Silent on success** — exit 0 means the fact holds, non-zero means it doesn't

Good commands check concrete, specific things:

```sh
# Dependency exists in manifest
grep -q '^clap' Cargo.toml
grep -q '"express"' package.json

# File or directory exists
test -f tests/cli.rs
test -d src/components

# A specific value or setting in config
jq -e '.scripts.test' package.json >/dev/null
grep -q 'edition = "2024"' Cargo.toml

# Build or test suite passes
cargo build --quiet 2>/dev/null
npm test --silent

# A property holds (or does not hold) across the codebase
! grep -rq 'unsafe' src/
! grep -rq 'unwrap()' src/handlers/

# Count-based invariants (use ranges, not exact numbers)
test $(find src -name '*.rs' | wc -l) -ge 10

# Behavioral test — actually exercise the tool
facts list --help 2>&1 | grep -q '\-\-section'
echo '- test fact' | facts lint /dev/stdin 2>/dev/null
```

#### What makes a command useless

The most common failure mode is **keyword grepping**: picking a word from the fact label and checking that it appears somewhere in a source file. This doesn't validate the fact — it validates that the codebase uses similar vocabulary.

```sh
# BAD: "heading depth maps to hierarchy"
grep -q "depth" src/parser.rs
# This checks that the word "depth" appears in the file. It would pass
# even if depth handling was completely broken. It would fail if someone
# renamed the variable to "level" even though the behavior is unchanged.

# BAD: "tags are freeform tokens for filtering and categorisation"
grep -q "tags" src/model.rs
# The word "tags" will always be in a file that deals with tags.
# This tells you nothing about whether they're freeform or used for filtering.

# BAD: "commands run sequentially"
! grep -q "async\|tokio" src/check.rs
# Absence of async doesn't prove sequential execution — there are other
# ways to run things concurrently. And this would still pass if someone
# added parallelism via std::thread.

# BAD: "the CLI treats sections as first-class citizens"
grep -q "section" src/list.rs
# What does this even check? That the word "section" appears? Of course it does.
```

The pattern to watch for: if your command is `grep -q "<keyword from the fact>" <file that obviously contains that keyword>`, it's not a real check. Stop and either find a meaningful command or leave the fact manual.

#### When to leave facts manual

Not every fact can or should have a command. Leave facts manual when they are:
- **Subjective or qualitative** — "extreme simplicity", "codebase is DRY", "polished UX"
- **About human processes** — "bump version, commit, push"
- **About external systems** you can't query locally
- **About behavior** that would require a complex integration test to verify and is already covered by the project's test suite
- **About design intent** — "each fact is atomic and independent", "file order is canonical"
- **Only checkable via keyword grep** — if the only command you can write checks for a keyword rather than the actual claim, leave it manual

A fact sheet with 30 genuinely validated facts and 20 honest manual facts is far more useful than one with 50 commands that are all `grep -q "<word>" <file>`.

### 4. Add missing facts

Identify important truths not yet in the fact sheet. Add them with `facts add`:

```
facts add "the project uses PostgreSQL for persistence" --section architecture \
  --command "grep -q 'postgres' docker-compose.yml"
facts add "project builds successfully" --command "cargo build --quiet" --section ci
```

Prefer facts that are:
- **Atomic** — one truth per fact
- **Verifiable** — include a check command when one can meaningfully validate the claim
- **Stable** — unlikely to change with every commit
- **Useful** — helps an agent understand or validate the project

Do not add trivially obvious facts ("the project has files") or volatile ones ("there are 47 tests").

### 5. Organize

Group related facts into sections using `--section`. Section paths support nesting (e.g. `api/auth`, `cli/subcommands`). Keep sections focused — split broad ones.

### 6. Validate and report

Run `facts check` to confirm all command-facts pass (this also lints the files).

Report what changed: facts added, edited, removed, commands added. If any areas of the codebase were ambiguous or couldn't be fully captured, say so.

## Guidelines

- Keep fact labels concise and declarative.
- **Command quality matters more than command count.** A command that doesn't actually test the fact is worse than no command — it creates false confidence. Only add commands that would break if the fact became false.
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
facts edit x1z --command "cargo build --quiet"

# Found a new truth while reading code
facts add "API rate limits to 100 req/min per key" --section api/limits \
  --command "grep -q 'rate_limit.*100' src/middleware.rs"

# An old fact about Python is no longer true — project migrated to Rust
facts remove p2q

# Add commands where they meaningfully validate the claim
facts edit a3b --command "test -f docker-compose.yml"
facts edit c7d --command "grep -q '^serde' Cargo.toml"

# Leave facts manual when no meaningful command exists:
# "controllers follow thin-controller pattern" — subjective, no short check
# "errors bubble up via anyhow context" — grep would just find the word "anyhow"
# "file order is canonical" — design intent, not mechanically checkable

# Verify everything
facts check

# Report: 3 added, 1 edited, 1 removed, 2 commands added, 3 left manual (no meaningful check)
```
