---
name: facts
description: >
  Manage .facts files — atomic, validatable truth statements about a project.
  Install, check, list, add, edit, remove, and lint facts via the CLI.
  ALWAYS read this skill when the user mentions facts in any capacity.
---

# facts

A CLI for fact-driven development. You use it to specify what must be true about a project, then validate that reality matches.

Project: https://github.com/av/facts

## Installing

If `facts` is not installed, install it with one of:

```bash
curl -fsSL https://raw.githubusercontent.com/av/facts/main/install.sh | sh
```

```bash
npm install -g @aspect-build/facts
```

```bash
pip install facts-cli
```

Verify with `facts --version`.

## Core idea

A `.facts` file is a flat list of atomic truth statements about a project. Each fact can optionally have a shell command that verifies it. The fact sheet serves as both specification (what should be true) and documentation (what is true) — the difference is just which direction you're working from.

```
- the API returns JSON
- label: project builds
  command: cargo build
- label: tests pass
  command: cargo test
  tags: [ci, core]
```

That's the entire format. Plain strings for simple facts, mappings when you need a command, tags, or explicit ID. Allowed mapping keys: `id`, `label`, `command`, `tags` — nothing else.

## Essential commands

**See everything:**
```
facts list
facts list --tags "not implemented"
facts list --has-command
```

**Validate:**
```
facts check
facts check --tags "ci"
```
`check` is your primary feedback loop. It lints the files first (aborting on structural errors), then runs every command-fact and reports pass/fail/manual. Run it often. Exit 0 means all command-facts pass; manual facts don't affect the exit code.

**Add facts:**
```
facts add "users can sign up" --section features/auth
facts add "signup returns 201" --command "curl -s -o /dev/null -w '%{http_code}' localhost:3000/signup | grep 201" --section features/auth
```

**Edit facts:**
```
facts edit <id> --add-tag "implemented"
facts edit <id> --remove-tag "blocked"
facts edit <id> --label "corrected statement"
facts edit <id> --command "new check command"
```
Prefer `--add-tag` / `--remove-tag` over `--tags`. The latter replaces all tags silently — use it only when you intend a full replacement.

**Remove facts:**
```
facts remove <id>
```

**Scaffold a new project:**
```
facts init
```

Run `facts <command> --help` for the full flag reference.

## How facts work

**Files:** `.facts` is the default. Additional sheets use semantic names (`cli.facts`, `api.facts`). All `*.facts` files in the project root are discovered automatically.

**Sections:** Markdown headings (`#`, `##`, etc.) create hierarchical sections addressable by path (e.g. `cli/subcommands`). Sections are created when you add to them and removed when their last fact is deleted.

**Tags:** `@word` tokens for filtering. Inline for plain strings (`- some fact @mvp`), `tags:` key for mappings. Stripped from the label before display and ID hashing. Filter with boolean expressions: `--tags "mvp and not blocked"`.

**IDs:** Every fact gets a short ID (3+ chars) derived from its label hash. Stable as long as the label doesn't change. Use `--id` or `--new-id` to override.

**Validation:** Commands run via `$SHELL` (fallback `sh`) in the project root. Exit 0 = fact holds. Write commands that are fast and idempotent — they run on every check.

## Writing good facts

- **Atomic** — one truth per fact, independently verifiable
- **Declarative** — state what is true, not what to do ("uses PostgreSQL" not "set up PostgreSQL")
- **Stable** — shouldn't change with every commit ("tests pass" not "there are 47 tests")
- **Verifiable** — add a command when a simple check exists; manual facts are fine for things that need judgment

Good validation commands are fast, idempotent, and test one thing. Prefer `test -f`, `grep -q`, and short script checks over running full builds.

## Agent workflows

**Understand a project:**
```
facts list                              # read the full spec
facts check                             # see what holds and what doesn't
facts list --manual                     # see what needs human/agent judgment
```

**Track implementation progress:**
```
facts list --tags "not implemented"     # what's left to do
facts edit <id> --add-tag "implemented" # mark done
facts check                             # verify
```

**Maintain accuracy:**
```
facts check                             # find failing facts
facts edit <id> --label "corrected"     # fix inaccurate facts
facts remove <id>                       # remove obsolete facts
facts add "new truth" --section foo     # add discovered truths
```

## Companion skills

- **facts-discover** — scan the codebase and make the fact sheet match reality. Use when you need to bootstrap or update the fact sheet from existing code.
- **facts-implement** — read the fact sheet as a spec and implement all unimplemented facts in code. Use when the fact sheet is ahead of the codebase.
