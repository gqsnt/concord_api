# SOL caller task — independent public-document attempt

Review label: **4.4**, not a package version recommendation. Checkout inspected: `0dd655018892a8cae21479f1cd249d2c875f6c83`. Rust: `rustc 1.99.1 (58cf8f9c3 2026-10-13)`.

## Boundary and reading record

The caller task below was completed before opening any author conclusions or other reviewer evidence. No `FRICTION.md`, `MAINTENANCE.md`, `NOTES.md`, `PROVENANCE.md`, `DECISIONS.md`, `NEXT_REVIEW*` or `v4/extensions-real-apis/*.md` was opened during this stage.

Read public material: root `Cargo.toml`, `crates/concord/Cargo.toml`; `docs/QUICKSTART.md` lines 1–150; searches in `docs/*.md` for CSV, JSONL, raw responses and checkpoints; `docs/CALLER_GUIDE.md` raw-response/base-url descriptions; `crates/concord-pagination/README.md` and Cargo manifest; source rustdoc and signatures in `crates/concord-pagination/src/types.rs` (Page/Resume/Budget/Outcome) and `sequential.rs` (item checkpoints and resume example); `crates/concord-extras/src/csv/mod.rs` module documentation and format constructors; `jsonl/mod.rs`, `jsonl/policy.rs` option documentation and `jsonl/lines.rs` reader method documentation/signatures; `stream/{mod,error,source,upload}.rs` documentation/signature search; `examples/testserver/Cargo.toml` and `src/lib.rs` public API documentation/signatures. Source-file excerpts also exposed adjacent implementation, but no author conclusions or review evidence were consulted. No provider implementation was needed.

No AGENTS.md was found on the parent chain or in the searched repository inventory. The cloud-environment skill was read; no environment-status tool was exposed, and `/etc/codex/network-policy.json` did not exist. All Cargo commands used `--offline`; HTTP was loopback only.

Initial guessed files `crates/concord-extras/src/{csv,jsonl}.rs` did not exist; `rg --files` located the module directories. An initial tool command selected the not-yet-created scratch working directory and failed to launch; no consumer/compiler result came from it. The directory was then created from the repository root.

## Task implementation and result

External crate: `review/sol/task/`, its own `[workspace]`, path dependencies, default reqwest features disabled, explicit `json`, `csv`, `jsonl`. Build/test command from that directory:

```sh
CARGO_TARGET_DIR=../target cargo test --offline
```

`src/lib.rs` declares `response Csv<NoHeader, Delimiter<b'|'>>;` directly, plus `type Export = Csv<HasHeader, Semicolon>;` in a second endpoint. Both decode `Vec<Row>` against `concord-example-testserver` successfully, including the header media parameters. No DSL workaround or alias for the direct const-generic path was required.

The JSON Lines body crosses chunk boundaries within a string. In `RecordErrors::Yield` mode, the consumer gets two successful rows, one skipped nonterminal error, zero terminal errors, then EOF. It obtains a native response through `prepare().send_raw()`, and opens it with `JsonLinesReader`.

An opaque cursor containing `+/=&` and a space is transmitted as a query value. The consumer emits item 10, saves the item checkpoint (`skip = 1`), finishes mid-page, resumes with `resume_items`, and gets exactly `[11, 12, 13, 14]`. Server cleanup reports zero open sockets. The replay requirement is documented clearly; the fixture intentionally has stable pages.

## Compiler error and correction (verbatim)

First actual `cargo test --offline` failed because **I guessed** `Outcome::CallerStopped`, instead of reading the enum before writing that assertion. This is a reviewer error, not a Concord defect:

```text
error[E0599]: no variant, associated function, or constant named `CallerStopped` found for enum `concord_pagination::Outcome` in the current scope
  --> src/lib.rs:73:53
   |
73 |         assert_eq!(items.finish().outcome, Outcome::CallerStopped);
   |                                                     ^^^^^^^^^^^^^ variant, associated function, or constant not found in `concord_pagination::Outcome`

For more information about this error, try `rustc --explain E0599`.
error: could not compile `concord-sol-review-task` (lib test) due to 1 previous error
```

Read the documented `Outcome` enum and changed it to `Outcome::Stopped`. Second attempt passed: **1 test passed**, zero failures, doc-tests passed (zero examples in this consumer). Full build output is retained in `task-attempt-{1,2}.log`. Unused-import warnings come from imports used only by this consumer's test module.

## Surprises / workarounds

- None needed for the requested direct generic DSL path, alias, incremental line recovery or checkpoint resume.
- Core extensions are opt-in features, pagination is a separate dependency; manifests and module rustdoc make this discoverable.
- The requested work spans three distinct public routes (typed finite decoder, raw response plus incremental reader, closure-based paginator). This task required some public-source navigation but the APIs composed without an integration shim.
- This stage is now complete. Subsequent files/logs under `review/sol/` are implementation review and may use all evidence.

## Post-task review artifacts (after the reading boundary)

The task imports were moved into the test module and `cargo fmt` was applied only to the scratch crate. `consumer-final.log` records the final successful normal `cargo test --offline`: the original caller test plus a positive every-split consecutive-values test. Five review probes are explicitly ignored by default and intentionally fail when run with `--ignored`; these are separated from the completed caller task. Compile-failure binaries require `compile-probes` and individual `--bin` selection. Their full diagnostics are saved in the corresponding `*.log` files. `FINDINGS.md` lists commands, observations, controls and limitations; root suites were run with the build directory under `review/sol/` and `--offline --locked`. Existing integration tests create/manage their own temporary fixture/staging paths; no production source files or other worktrees were edited.
