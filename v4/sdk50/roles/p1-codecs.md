# P1 capsule: one `concord-codecs` package; `concord-extras` becomes a compatibility facade

You implement a bounded change in the Concord Rust workspace (your working directory, a git
worktree). Read `AGENTS.md`/`CLAUDE.md` if present, `docs/MAINTAINERS.md`, `scripts/verify.sh`,
`scripts/check-graph.py`, `scripts/check-ci.py`, `scripts/package-check.sh`, `scripts/consumers.sh`.
You cannot commit (`.git` is read-only); leave the finished change in the working tree. Do not
touch `crates/concord-core`, `crates/concord-macros` or any `examples/*-sdk` source except a
manifest line when strictly needed.

## Goal

The standard formats (Json, Text, Binary, Form, Csv, JSON Lines/NDJSON/consecutive values, SSE,
multipart, the `stream` infrastructure, `IgnoreContentType`, `Encoded`, media parsing) live today in
`crates/concord-extras`. Move them, once, into a new crate `crates/concord-codecs` (package name
`concord-codecs`, version.workspace, same features: `json`, `form`, `stream`, `csv`, `jsonl`,
`multipart`, `sse`, same optional dependencies). Use `git mv` semantics (move the files: src and
tests) so history follows. Then:

1. `concord-extras` keeps its name, version and **every public path**, as a facade: its `lib.rs`
   reexports the codecs crate (`pub use concord_codecs::*;` plus the public modules — `csv`,
   `jsonl`, `sse`, `stream`, `multipart`, and whatever else is public today). Its features forward
   (`json = ["concord-codecs/json"]`, …). No type, trait, marker or error is defined twice: old and
   new paths must name the same Rust item. It owns no new functionality; document it as the
   compatibility facade.
2. The `concord` facade depends on `concord-codecs` directly (optional, behind the existing
   `extras` feature and the same family features; keep every feature name and default). Its
   reexports (`concord::Json`, `concord::csv`, …) keep their paths.
3. A direct consumer can depend on `concord-codecs` with only the features it needs, without the
   macro crate and without `concord-extras`.
4. Keep the shared stream internals (`stream/records.rs` sealing, private `RecordEncoder`, media
   parsing) private inside `concord-codecs`. Do not unseal anything. Do not copy the driver.
5. Update the maintained gate coherently, without weakening any check:
   - `scripts/verify.sh`: the `test-extras-*`/`doc-extras-*` steps become the same checks on
     `concord-codecs` (each feature alone, none, all), plus a step testing the `concord-extras`
     facade alone (default-off and all-features) — keep step names readable (e.g.
     `test-codecs-csv`). Keep the `no_reqwest_multipart` guard with `concord-codecs` added.
   - `scripts/check-graph.py`: allowed edges and resolved-graph exclusions for `concord-codecs`
     (the old extras rules move to codecs: CSV alone has no serde_json, SSE no serde, multipart no
     serializer…), `concord-extras` may depend only on `concord-codecs`, `concord` on
     `concord-core`, `concord-codecs`, `concord-macros` (decide whether `concord` still needs
     `concord-extras`: prefer not). Keep the positive controls.
   - `scripts/check-ci.py`, `scripts/package-check.sh`, `scripts/consumers.sh`: add
     `concord-codecs` to every crate list in dependency order (core → codecs → extras → macros →
     concord …). In `consumers.sh` add packaged consumers under `checks/consumers/`:
     `codecs-json` (only `concord-codecs` with `json` + `concord-core`), `codecs-csv` (csv only:
     must not resolve serde_json), `codecs-sse` (sse only: no serde), and `extras-legacy` (old
     `concord_extras::…` imports compile and are the **same types** as `concord_codecs::…` — prove
     identity by passing a value of one path where the other is expected, e.g. a
     `concord_codecs::csv::CsvOptions` into a function taking `concord_extras::csv::CsvOptions`, a
     `concord_codecs::Json` marker as `concord_extras::Json`, a reader/error type likewise). Follow
     the existing consumer layout and forbidden-package checks.
   - `Cargo.toml` workspace members; `docs/MAINTAINERS.md` runtime map; crate READMEs; the
     `concord-extras` README states it is a compatibility facade and points to `concord-codecs`.
   - Any test or record checker that lists crate names.
6. Move the extras tests with the code (they test the codecs). Keep their assertions unchanged.

## Verification you run (bounded; one cargo job at a time, `CARGO_BUILD_JOBS=6`)

- `cargo test --locked -p concord-codecs` with each feature alone and `--no-default-features`.
- `cargo test --locked -p concord-extras --no-default-features` and `--all-features`.
- `cargo test --locked -p concord` default and `--no-default-features --features csv,jsonl,sse`.
- `python3 -I scripts/check-graph.py`, `python3 -I scripts/check-ci.py --workflow`,
  `python3 -I -B -m unittest discover -s scripts/tests -p "test_*.py"`.
- `scripts/consumers.sh` and `scripts/package-check.sh` if they run in reasonable time; report.
- `cargo clippy --locked -p concord-codecs --all-features --all-targets -- -D warnings`,
  `cargo doc` with `RUSTDOCFLAGS="-D warnings"` for codecs/extras.
Do not run the whole `scripts/verify.sh` (the coordinator runs the full gate).

## Report (final message)

Files moved/changed, the facade's public surface, feature mapping table, every gate script change
and why, the commands you ran with their exit status and test counts, anything you could not
make pass, and any place where a single definition could not be kept (there should be none).
