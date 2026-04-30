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
