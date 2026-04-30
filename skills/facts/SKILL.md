# facts

You have access to `facts`, a CLI for fact-driven development.

## What are fact sheets?

A `.facts` file is a flat list of validatable atomic facts about a project. Each fact is a single truth statement that can optionally be verified by a shell command. Fact sheets live in the project root (the directory containing `.git`).

The format is simultaneously valid Markdown and valid YAML (per-section). Lines are either headings (`#` prefixed), facts (`-` prefixed), or blank.

## File conventions

- `.facts` is the default/main fact sheet
- Additional sheets use semantic names (e.g. `cli.facts`, `api.facts`)
- All `*.facts` files in the project root are discovered automatically

## Fact formats

Plain string fact:
```
- the API returns JSON
```

Mapping fact with validation command:
```
- label: project is a Cargo project
  command: test -f Cargo.toml
```

Full mapping with all keys:
```
- id: custom-id
  label: tests pass
  command: cargo test
  tags: [ci, core]
```

Allowed mapping keys: `id`, `label`, `command`, `tags`. No others.

## Tags

Tags are `@word` tokens for filtering and categorisation.

Inline syntax (plain strings only):
```
- the API is RESTful @api @core
```

Mapping syntax:
```
- label: tests pass
  command: cargo test
  tags: [ci, core]
```

Tags are stripped from the label before display and ID computation.

## Identity

Every fact has a short ID (3+ characters) derived from a hash of its label. IDs are computed, not stored. An explicit `id` key in a mapping overrides the generated ID. IDs are stable as long as the label does not change.

## Sections

Headings create hierarchical sections addressable by path (e.g. `cli/subcommands`). Sections are created when you add facts to them and removed when their last fact is deleted.

## Subcommands

### `facts` (bare, no subcommand)

Defaults to `facts list`. Shows all facts in file order.

### `facts list`

Show facts in file order with ID and section path.

```
facts list
facts list --section cli
facts list --has-command
facts list --manual
facts list --tags "mvp and not blocked"
facts list --file cli.facts
```

Flags:
- `--file` — filter by file name
- `--section` — filter by section path
- `--has-command` — only facts with a validation command
- `--manual` — only facts without a validation command
- `--tags` — boolean tag filter expression (supports `and`, `or`, `not`, parentheses)

### `facts check`

Run all command-facts and report pass/fail/manual. Lints all files first — check aborts early on lint errors.

```
facts check
facts check --tags "ci"
facts check --timeout 30
```

Flags:
- `--tags` — boolean tag filter expression
- `--timeout` — per-command timeout in seconds

Exit code: 0 if all command-facts pass, non-zero if any fail. Manual facts do not affect the exit code.

### `facts add <label>`

Append a fact to a file and section.

```
facts add "the API returns JSON"
facts add "tests pass" --command "cargo test" --section ci
facts add "uses Express" --file api.facts --tags "core,api"
facts add "custom entry" --id my-id
```

Flags:
- `--section` — target section path (created if needed, supports nested paths like `cli/subcommands`)
- `--file` — target `.facts` file (default: `.facts`, created if needed)
- `--command` — validation command
- `--id` — explicit ID override
- `--tags` — comma-separated tags

### `facts remove <id>`

Remove a fact by its ID. Prints the removed fact. No confirmation prompt.

```
facts remove abc
```

### `facts edit <id>`

Modify a fact by its ID.

```
facts edit abc --label "new label"
facts edit abc --command "new command"
facts edit abc --tags "tag1,tag2"
facts edit abc --add-tag "implemented"
facts edit abc --remove-tag "blocked"
facts edit abc --new-id custom-id
```

Flags:
- `--label` — new label text
- `--command` — new validation command
- `--new-id` — new explicit ID
- `--tags` — new tags (comma-separated, replaces all existing tags)
- `--add-tag` — add tags without removing existing ones (comma-separated)
- `--remove-tag` — remove specific tags (comma-separated)

`--tags` cannot be combined with `--add-tag` or `--remove-tag`.

Plain string facts are promoted to mappings when they gain a command, id, or tags via edit.

### `facts lint`

Validate that fact sheets are parseable. Does not run validation commands.

```
facts lint
facts lint --file cli.facts
```

Flags:
- `--file` — lint a specific file instead of all `*.facts` files

### `facts init`

Scaffold a `.facts` file. Detects well-known framework/runtime combos (Cargo, Node.js, Python, Go, Ruby, Java, etc.) and generates starter facts with validation commands.

```
facts init
```

Refuses to overwrite an existing `.facts` file.

## Common workflows

### Start a new project with facts

```
facts init
facts list
facts check
```

### Add a new requirement

```
facts add "users can sign up" --section features/auth
facts add "signup endpoint returns 201" --section features/auth --command "curl -s -o /dev/null -w '%{http_code}' localhost:3000/signup | grep 201"
```

### Filter and review

```
facts list --tags "mvp"
facts check --tags "mvp and not blocked"
facts list --manual --section api
```

### Clean up

```
facts remove abc
facts edit def --label "updated requirement"
facts lint
```
