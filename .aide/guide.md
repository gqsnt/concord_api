# Rust work guide

Operational facts for working in a Rust workspace. Project files win over this guide.

## Before changing code
- Read the project's own instructions first: README, CONTRIBUTING, `justfile`/`Makefile`/xtask, CI files, `rust-toolchain.toml`.
- The toolchain comes from `rust-toolchain.toml` through rustup. Check it with `rustc -V` and `cargo -V` in the workspace; never change the pin to make something build.
- Locate code with `cargo metadata --format-version 1 --no-deps`, `rg`, and the crate's `lib.rs`/`mod.rs` tree. For a dependency, read the source of the version in `Cargo.lock` (under `~/.cargo/registry/src/`), not another version's docs.

## Commands
- Prefer the project's recipes (`just <recipe>`, `cargo xtask ...`) for final checks; use the narrowest Cargo command while iterating: `-p <crate>`, `--lib`, `--test <name>`, the exact `--features` set.
- Pass `--locked` when the project does; never update `Cargo.lock` unless the task needs it.
- Feature- and target-gated code needs the matching `--features`/`--no-default-features`/`--target` to be checked at all.
- `cargo nextest run` does not run doctests; run `cargo test --doc` separately when docs or examples change.
- Compile-fail (UI) tests compare compiler output: after an intended diagnostic change, review the new `.stderr` before accepting it.
- Keep long command output in a file (`cmd > /tmp/x.log 2>&1; echo exit=$?`) and read the first error, not the tail. Report the real exit status.
- One Cargo build per target directory at a time: "Blocking waiting for file lock" means another build holds it. Don't start duplicate full builds; wait, or narrow the command.

## Finishing
- Run formatting (`cargo fmt --all -- --check`), the lints the project uses (often `cargo clippy ... -- -D warnings`) and the tests that cover the change, on the final source.
- If the source changes after a check, that check's result no longer applies.
- Report what you ran, its exit status, and anything you could not run and why.
