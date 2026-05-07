# facts

Rust CLI that manages `.facts` files — flat lists of atomic, validatable truth statements about a project. Serves as both specification and documentation.

Read `.facts` in the project root for the full format spec, CLI reference, and architecture.

## Commands

```sh
cargo build              # build
cargo test               # run all tests (integration + inline unit)
cargo clippy             # lint
cargo fmt --check        # check formatting
```

Single test:

```sh
cargo test test_name -- --nocapture
```

## Code style

Minimal Rust. Two dependencies: `clap` (CLI) and `anyhow` (errors). No async, no FFI, single-threaded.

```rust
fn cleanup_empty_sections(sections: &mut Vec<Section>, path: &[usize]) {
    for depth in (0..path.len()).rev() {
        let current_path = &path[..=depth];
        let section = locate::navigate_to_section(sections, current_path);
        if section.facts.is_empty() && section.children.is_empty() {
            if depth == 0 {
                sections.remove(current_path[0]);
            } else {
                let parent_path = &current_path[..current_path.len() - 1];
                let parent = locate::navigate_to_section_mut(sections, parent_path);
                parent.children.remove(current_path[depth]);
            }
        } else {
            break;
        }
    }
}
```

## Testing

All user-facing features are tested end-to-end in `tests/cli.rs`. Each test gets an isolated temp directory with a `.git` marker. Unit tests are inline in their modules.

## Releasing

1. Bump version in `Cargo.toml`
2. Commit and push
3. `git tag vX.Y.Z && git push origin vX.Y.Z`
4. GitHub Actions builds binaries for 5 targets and creates the release

<!-- facts:start -->
## Fact-driven development

This project uses [facts](https://github.com/av/facts) for specification and documentation. All work flows through the fact sheet — it is the source of truth.

**Read the skill first:** `facts skills show facts` — it has the full workflow, format spec, and command reference.

**All tasks start and end with facts.** When the user asks you to implement, build, fix, or change anything:
1. `facts list` — read the spec to orient
2. Add or refine facts that describe what should be true when the work is done
3. Implement the code changes
4. Verify only what you changed — use `facts check --tags "<tag>"` or `facts get <id>` to scope checks to the facts you worked on. Never run bare `facts check` unless asked.
5. Tag completed facts `@implemented`

Do not wait for the user to mention facts. Define facts for every task, refine them, implement against them, and verify.

**Lifecycle:** `@draft` → `@spec` → `@implemented`

**Skills** (invoke via `facts skills show <name>`):
- `facts-refine` — sharpen `@draft` facts into `@spec` with the user
- `facts-discover` — scan the codebase and sync facts to reality (only when explicitly asked)
- `facts-implement` — implement `@spec` facts in code, verify, tag `@implemented`
<!-- facts:end -->
