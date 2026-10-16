# Contributing to Poof

Thanks for helping. Poof aims to be the storage tool Mac users can trust, so
correctness and safety come before features.

## Development setup

```sh
rustup toolchain install stable --component clippy rustfmt
cargo run -p poof                      # run the app (debug)
cargo test --workspace                 # all tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
```

`cargo run --release -p poof-core --example report [path]` prints a storage
report without the GUI, which is handy when working on the core.

> **Full Disk Access is granted per app.** When you run from a terminal, Poof
> inherits the terminal's permissions. To test permission handling, build the
> bundle (`ARCHS=aarch64 scripts/bundle.sh`) and launch `dist/Poof.app`.

## Ground rules

- **Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.** Domain logic
  goes in `poof-core`. `poof-app` only renders and orchestrates.
- **Keep it simple and DRY.** Reuse widgets from `widget/` and tokens from
  `style.rs`; don't hard-code colours or spacing in views.
- **Test the core.** New logic in `poof-core` needs unit tests, using
  `tempfile` for anything that touches the file system.
- **Safety-sensitive changes** (anything in `remove.rs`, the cleanup catalog or
  leftovers detection) need a clear rationale in the PR. Mark a rule
  `Safety::Safe` only if its data is regenerated automatically with no user
  impact.
- CI must be green: `fmt`, `clippy -D warnings`, tests and a bundle build.

## Adding a cleanup rule

Append a `Rule` to `crates/poof-core/src/cleanup/catalog.rs`:

```rust
Rule {
    id: "my-tool-cache",                 // unique, kebab-case
    name: "My Tool cache",
    description: "What it is, and why removing it is fine.",
    category: Category::Developer,
    safety: Safety::Safe,                // or Review
    source: Source::Paths(&["~/Library/Caches/my-tool"]),
    permanent: false,
},
```

Overlaps with broader rules are resolved automatically.

## Commit messages

We use [Conventional Commits](https://www.conventionalcommits.org) so the
changelog can be generated with [git-cliff](https://git-cliff.org):

```
feat(cleanup): add Bun cache rule
fix(scan): count hard links once
docs: explain purgeable space
```

## Releasing (maintainers)

Poof follows [Semantic Versioning](https://semver.org).

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and run
   `cargo check` to refresh `Cargo.lock`.
2. Update `CHANGELOG.md` (`git cliff --unreleased --tag vX.Y.Z --prepend CHANGELOG.md`
   helps), then commit with `chore(release): vX.Y.Z`.
3. Tag and push: `git tag -a vX.Y.Z -m "Poof vX.Y.Z" && git push --follow-tags`.
4. The **Release** workflow builds the universal app, creates the GitHub
   release with that version's changelog section, and attaches the `.dmg`,
   `.zip` and `SHA256SUMS.txt`.
