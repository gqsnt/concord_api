# Validation record — 2026-11-07

All commands ran from the worktree root, serially, using default `target/` and
`CARGO_BUILD_JOBS=4`. No relative `CARGO_TARGET_DIR` was set. No real-provider HTTP request was
made. Live provider calls: **NOT_RUN**.

## Resolution and diagnostics

| Command/action | Observed result |
|---|---|
| `git rev-parse HEAD` | Frozen baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`. |
| Initial `cargo test --offline -p concord-sdk-mediawiki-action-api` | Failed dependency lookup: TestServer package is named `concord-example-testserver`, not `concord-testserver`. Fixed the local dependency alias. |
| Next `cargo test --offline -p concord-sdk-mediawiki-action-api` | Cargo added only the SDK lock entry and compiled dependencies offline; SDK compile failed on `Resume::Key`. Changed it to documented `Resume::At`. |
| First full `cargo test --locked --offline -p concord-sdk-mediawiki-action-api` | 10 protocol tests passed; the null-continuation case failed because a present null became exhaustion. Added strict presence decoding. |
| Next locked offline test run | 11 protocol and 6 workflow tests passed. |
| Initial locked offline Clippy/rustdoc | Both passed; final exact command forms were run after formatting and adding public docs. |
| `rustfmt --edition 2024` on this package's Rust files | Completed; no other package was formatted. |

## Final required gates

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-mediawiki-action-api
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-mediawiki-action-api --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-mediawiki-action-api --no-deps
```

All three exited **0**. Test result: **17 integration tests passed** (11 protocol, 6 workflows),
plus **1 README doctest compiled** without execution. No ignored or failed tests. Clippy and
rustdoc emitted no warnings. Generated docs:
`target/doc/concord_sdk_mediawiki_action_api/index.html`.

Inspected the generated search `send()` signature in
`target/doc/concord_sdk_mediawiki_action_api/struct.MediaWikiQuerySearch.html`; it exposes
`impl Future<Output = <QueryPolicy as SendWith<Envelope<SearchResults>>>::Output> + Send`.
This confirms friction F3 from rendered generated usage rather than guessing the macro output.

## Scope and counts

A Python `tomllib` comparison of `git show HEAD:Cargo.lock` with the final lockfile established
that removing the new SDK entry leaves **every existing package entry identical**, including
versions and dependency lists. `git diff --check` passed. Final git status contains only root
`Cargo.toml`, root `Cargo.lock` and the new `examples/mediawiki-action-api-sdk/` directory.

| Files | Physical lines, including comments/blank lines |
|---|---:|
| `src/lib.rs`, `models.rs`, `error.rs`, `policy.rs`, `pagination.rs` | 461 |
| `tests/protocol.rs`, `workflows.rs`, `support/mod.rs` | 691 |
| 14 files in `fixtures/` | 47 |

Inline synthetic protocol cases are counted under tests. Documentation and manifests are
excluded from these three counts. No commit was attempted.
