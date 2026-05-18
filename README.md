<div align="center">

<img src="assets/readme/hero.png" alt="facts" width="800" />



https://github.com/user-attachments/assets/80981efb-8bab-47ee-a567-3eb6071321ad



Read your entire project spec in 30 seconds. Verify it in one command.

[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/av/facts)](https://github.com/av/facts/releases)

</div>

Your project has 48 facts, 31 of them implemented: code-backed, verified by command. 12 are specs your agent is working through. 5 are rough drafts you'll refine later. You know all of this because you ran `facts check`.

```
# auth
- users authenticate via OAuth2 @implemented
- sessions expire after 24 hours @implemented
- failed logins rate-limited to 5 per minute @spec
- label: bcrypt password hashing with cost factor 12
  command: grep -q 'bcrypt.*12' src/auth.ts

# data
- all timestamps stored in UTC @implemented
- soft deletes only, no rows dropped @spec
- PII encrypted at rest @draft

# api
- REST with versioned endpoints @implemented
- rate limiting on all public routes @spec
- structured error responses with error codes @spec
```

That's a `.facts` file, where each line is one atomic claim about your project. Tags track where each fact is in its lifecycle: `@draft` (rough idea), `@spec` (precise, ready to build), `@implemented` (true, code-backed). Your agent manages the transitions. The format is a flat list of claims: short enough to read in full, structured enough to manage, and when a fact has a shell command, the machine verifies it so the agent doesn't have to.

## Install

Give your agent the facts skill:

```sh
npx skills add av/facts
```

Then ask it to **Init facts**. It detects your stack, creates a `.facts` file with initial project truths, and sets up the full workflow.

<details>
<summary>Manual install</summary>

```sh
curl -fsSL https://av.codes/facts.sh | sh
```

```sh
npm install -g @avcodes/facts        # or npm
pipx install facts-cli               # or pipx
```

It's a single Rust binary with two dependencies, running on Linux, macOS, and Windows.

Then scaffold your project:

```sh
cd your-project
facts init              # scaffold .facts + install skills
facts init api          # create api.facts instead
facts check
```

</details>

## Agent skills

Four skills ship with every install. Your agent uses them to manage the full lifecycle without you directing every step.

| Skill | What it does |
|-------|-------------|
| **facts** | Core operations: read the spec, check it, add and edit facts |
| **facts-discover** | Scan the codebase, classify every fact by lifecycle stage, add missing truths |
| **facts-refine** | Pick up `@draft` facts, sharpen them into precise `@spec` facts with you |
| **facts-implement** | Pick up `@spec` facts, build them in code, verify, tag `@implemented` |

The workflow is a loop: you write rough ideas as `@draft`. The agent refines them into specs. Then it implements them, runs `facts check`, and tags what it built. You see the progress in the fact sheet, right there in the spec itself.

```
@draft → @spec → @implemented
```

Every fact moves through this pipeline. At any point you can run `facts check` and know exactly where your project stands: what's done, what's in progress, and what's still just an idea.

---

## How it works

<img src="assets/readme/howto-1.png" alt="1. Describe" width="700" />

**Describe** what should be true by writing claims as plain strings, one fact per line. Use `#` headings to organize by domain, tag each fact with its lifecycle stage, and for facts the machine can verify, add a `command` that exits 0 when the claim holds.

<img src="assets/readme/howto-2.png" alt="2. Verify" width="700" />

**Verify** your facts with `facts check`, which lints all files, runs every command, and groups results by status: green pass, red fail, yellow manual. Facts without commands are verified by the agent against the codebase. It exits 0 when everything passes, non-zero when anything fails. Plug it into CI or let your agent run it after every change.

<img src="assets/readme/howto-3.png" alt="3. Implement" width="700" />

**Implement** against the spec: your agent reads the fact sheet to understand the project, picks up `@spec` facts, builds them, and runs `facts check` to verify its own work. When a fact passes, it tags `@implemented`, and the spec updates itself as the project evolves.

---

## The format

A `.facts` file is valid Markdown *and* valid YAML per section.

```
# section
- a plain string fact @tag
- label: a fact with a check command
  command: test -f src/main.rs
  tags: [core, mvp]
```

| Key | Required | Purpose |
|-----|----------|---------|
| `label` | yes | The claim |
| `command` | no | Shell command, exit 0 = true |
| `tags` | no | Freeform tokens for filtering |
| `id` | no | Override the auto-generated ID |

**Tags** filter with boolean expressions: `--tags "core and not blocked"`. Three well-known tags (`@draft`, `@spec`, `@implemented`) drive the lifecycle, but any tag works.

**Files:** `.facts` is the default. Additional sheets use semantic names (`cli.facts`, `api.facts`). All `*.facts` files in the project root are discovered automatically. The `--file` flag accepts subdirectory paths (e.g. `--file src/api`) — subdirectory files are only visible when explicitly targeted.

**Sections** use Markdown headings. Nesting creates hierarchy addressable by path (`api/auth`). Created when you add to them, removed when empty.

**IDs** are short hashes of the label, stable as long as the label doesn't change.

---

## Commands

Common short aliases (all extra args are passed through to the real command):

- `ll` = `list --light`  `ls` = `list`
- `rm` = `remove`
- `at <id> <tag>` = `edit <id> --add-tag <tag>`  `rt <id> <tag>` = `edit <id> --remove-tag <tag>`

```
facts                                    # list all facts (default)
facts ll                                 # list in markdown-like skim format
facts ll --tags "draft"                  # combine alias + filter
facts check                              # verify everything
facts check --tags "mvp and not blocked" # filter by tag expression
facts add "claim" --section api          # add a fact
facts at <id> spec                       # quick tag add (or use the full edit form)
facts rt <id> draft                      # quick tag remove
facts remove <id>                        # remove a fact
facts get <id>                           # look up a single fact
facts move <id> --section new/path       # relocate a fact
facts list --section api/auth            # filter by section
facts lint                               # validate structure
facts fmt                                # normalize all files
facts init                               # scaffold .facts + install skills
facts init api                           # create api.facts instead
facts add "claim" --file src/api         # add to a subdirectory file
facts uninit                             # remove facts from project
```

---

## Dogfooding

This repo uses a `.facts` file to describe itself: 224 facts, 154 verified by command, none failing.

```
$ facts check
...
154 passed, 0 failed, 70 manual
```

Clone the repo, install facts, and run `facts check` to see it work on itself.

---

## License

MIT
