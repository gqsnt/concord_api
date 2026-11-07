# Validation — 2026-11-07

Live provider calls: **NOT_RUN**. All executed HTTP requests targeted local
`examples/testserver` listeners. Rustdoc examples are `no_run` compile checks.

Environment: frozen baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`,
`CARGO_BUILD_JOBS=4`, one cargo command at a time, default worktree `target/`.
No `CARGO_TARGET_DIR` override was set. No commit was attempted.

| Command | Result |
|---|---|
| `cargo test --offline -p concord-sdk-strava --no-run` | PASS; initial unlocked offline build; only the new local package added to the lockfile |
| `cargo fmt -p concord-sdk-strava` | PASS; formatted only the owned package |
| `cargo test --locked -p concord-sdk-strava --test protocol` | PASS; initial 10 protocol tests |
| `cargo test --locked -p concord-sdk-strava` | Final PASS; 12 protocol tests, 10 workflow tests, 3 compile-only rustdoc examples; no failures or ignored tests |
| `cargo clippy --locked -p concord-sdk-strava --all-targets -- -D warnings` | Final PASS; warnings denied, no lint suppressions added |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-strava --no-deps` | PASS; generated `target/doc/concord_sdk_strava/index.html` |
| `git diff --check` | PASS |

Observed intermediate diagnostics, all resolved: workflow compilation E0609 for
`Incomplete.items` (correct field: `collected`); Clippy `type_complexity` on the
pagination return signature; E0308 while introducing the page-metadata alias
(the adapter still returned `Response` metadata until its projection was updated).
These were package-local corrections. No shared crate was changed.

The final test suite exercises:

- Authorized activity pages through the empty terminator, a short intermediate page, activity detail and 64-bit IDs.
- Page cap/resume, prefix preservation on provider error, and unknown sport values.
- OAuth code form encoding, expired-token refresh, shared concurrent acquisition, 401 generation invalidation, successive refresh-token rotation and failed acquisition with no API dispatch.
- Lazy/non-cloneable multipart sources, exact file bytes and fields, upload acceptance, pending/complete polls and retrieval of the resulting activity.
- Ambiguous timeout after the whole activity file reached the fixture; exactly one source read, one POST, one connection, no poll or replay.
- Body-source failure, unreadable upload receipts, processing failure, poll cap/resume, changed receipt ID and one-second status retry spacing.
- HTTP status classes and both general-problem and upload-status error envelopes.
- Malformed/truncated/oversized/wrong-media/missing-media bodies on success and failure; HTTP status survives all error-document failures.
- Short/daily and overall/non-upload 429 combinations, strict pair parsing, Retry-After/unexplained-429 gating, and upload exemption from non-upload quotas.
- Secret redaction across prepared/generated requests, credentials, tokens, provider errors, upload states and response metadata.
- Transient read retries capped at two; body errors, token grants, writes and 429s do not trigger replay.

Final source inventory (physical lines, including documentation and blank lines):

| Area | Files | Lines |
|---|---:|---:|
| `src/` | 5 | 1031 |
| `tests/` including `tests/support/mod.rs` | 3 | 1076 |
| `fixtures/` | 15 | 24 |

A structural comparison of baseline and final Cargo.lock found exactly one added
package (`concord-sdk-strava` 0.1.0), no removed package, and no changed pre-existing
entry. Tracked diffs are only root `Cargo.toml` and `Cargo.lock`; the new package is
untracked worktree content ready for review. The root manifest change adds only
`examples/strava-sdk` to members.

## Maintenance — activity update (2026-11-07)

The independent maintainer's change (`PUT /activities/{id}`) and its validation are recorded
in `MAINTENANCE.md`; the results above are the author's and were not re-labelled. After the
change: 25 first-use, 17 protocol, 4 update-consumer, 10 workflow tests and 4 compile-only
rustdoc examples pass; Clippy and rustdoc with warnings denied pass. Live provider calls:
**NOT_RUN**.
