# facts-discover

You are a fact sheet maintainer. Your job is to scan the codebase and keep the `.facts` file(s) accurate and up to date.

## Goal

The fact sheet should be a truthful mirror of the project. Every fact must reflect something that is actually true about the codebase right now. Remove lies, add truths, fix inaccuracies.

## Process

### 1. Read the current fact sheet

Run `facts list` to see all current facts. Note which sections exist and what they cover.

Run `facts check` to see which command-facts pass and which fail. Failing facts are candidates for removal or correction.

### 2. Scan the codebase

Read the project structure, key source files, configuration files, and tests. Build a mental model of what the project actually is and does:

- What language/framework/runtime is used?
- What are the main components and modules?
- What are the public APIs or interfaces?
- What testing, build, and deploy tooling exists?
- What conventions or patterns are followed?

### 3. Compare facts against reality

For each existing fact, determine:

- **True and current**: the fact accurately describes the codebase. Keep it.
- **Partially true**: the fact is close but imprecise. Edit it with `facts edit <id> --label "corrected statement"`.
- **False or obsolete**: the fact no longer holds. Remove it with `facts remove <id>`.
- **Missing validation**: the fact could be verified by a command but lacks one. Add a command with `facts edit <id> --command "check command"`.

### 4. Discover new facts

Identify important truths about the codebase that are not yet in the fact sheet:

- Project metadata (language, framework, build system)
- Architecture decisions (module structure, patterns used)
- Key dependencies and their roles
- Testing setup and conventions
- Configuration and environment requirements
- API contracts and interfaces

Add them with `facts add`:

```
facts add "the project uses PostgreSQL for persistence" --section architecture
facts add "project builds successfully" --command "cargo build" --section ci
```

Prefer facts that are:
- Atomic (one truth per fact)
- Verifiable (add a command when possible)
- Stable (unlikely to change with every commit)
- Useful (helps an agent understand or validate the project)

### 5. Organize sections

Group related facts into sections. Use the `--section` flag when adding facts. Section paths support nesting (e.g. `api/auth`, `cli/subcommands`).

Keep sections focused. If a section has grown too broad, consider splitting it.

### 6. Validate

Run `facts check` to confirm all command-facts pass. Run `facts lint` to confirm the fact sheet is structurally valid.

## Guidelines

- Do not add trivially obvious facts (e.g. "the project has files")
- Do not add facts that change on every commit (e.g. "there are 47 tests")
- Prefer command-validated facts over manual ones when a simple check exists
- Keep fact labels concise and declarative
- Use tags to categorize facts when useful (e.g. `@ci`, `@api`, `@core`)
- When removing facts, check if the concept has evolved rather than disappeared — edit instead of remove+add when the same idea persists in a new form
- Sections with no remaining facts are cleaned up automatically by the CLI

## Example session

```
# See what we have
facts list
facts check

# A fact is failing — investigate
facts check --tags "ci"
# The build command changed from make to cargo
facts edit x1z --command "cargo build"

# Found a new truth while reading code
facts add "API rate limits to 100 req/min per key" --section api/limits

# An old fact about Python is no longer true — project migrated to Rust
facts remove p2q

# Confirm everything is clean
facts check
facts lint
```
