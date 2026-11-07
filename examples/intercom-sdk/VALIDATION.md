# SDK29 validation receipt

Date: **2026-11-07**. Baseline HEAD:
`8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`.
Package: `concord-sdk-intercom` 0.0.0. Toolchain:
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`, edition 2024.
Live provider calls: **NOT_RUN**. No commit was attempted.

## Commands and observed results

All Cargo commands ran one at a time from the worktree root with
`CARGO_BUILD_JOBS=4`, using the default worktree `target/`. No CARGO_TARGET_DIR
override was set. No new registry dependency was required.

| Command | Observed result |
| --- | --- |
| `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-intercom --no-run` | Exit 0; initial unlocked offline build wrote only the SDK lock entry; 15.35s. Library built before integration test files were added |
| `CARGO_BUILD_JOBS=4 cargo test --locked --offline -p concord-sdk-intercom` | Exit 0; first integration run: 11 protocol + 7 workflow tests. Two missing crate-doc warnings were observed and fixed |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked --offline -p concord-sdk-intercom --all-targets -- -D warnings` | Exit 0, 11.32s; no warnings |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-intercom` | Exit 0 after error-envelope/cursor refinements: 12 protocol + 7 workflow tests, plus 1 compile-only README doctest; no warnings. Repeated successfully after final public reexport/doc adjustment |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-intercom --all-targets -- -D warnings` | Exit 0 on final source, 0.37s; no warnings |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-intercom --no-deps` | Exit 0 on final source, 3.61s; generated target/doc/concord_sdk_intercom/index.html |
| `rustfmt --check --edition 2024 examples/intercom-sdk/src/*.rs examples/intercom-sdk/tests/*.rs examples/intercom-sdk/tests/support/mod.rs` | Exit 0; formatting clean |
| `git diff --check` | Exit 0 |
| Python `tomllib` comparison of `git show HEAD:Cargo.lock` and working lock | Every existing package entry, version, checksum and dependency list is identical; only concord-sdk-intercom 0.0.0 added |
| Python manifest and git path audit | Root manifest differs only by the permitted member; tracked edits are Cargo.toml/Cargo.lock, all new files are under examples/intercom-sdk; no shared source changes |

Checks exercise both connected workflows, cursor encoding and page caps,
prefix/failed-key preservation, all 37 synthetic error-matrix cases, 20 body
damage combinations, malformed envelopes, header parsing, native declared body
limits, request redaction, no redirects and no automatic create replay.
The required failure returns a typed headerless 429 with Missing reset and
Missing Retry-After after exactly two page requests, retaining one contact;
the create-side 429 and ambiguous disconnect each produce one request.

The two workflow tests are fixture evidence, not live compatibility claims.
HTTP/1.1 on loopback is covered; live TLS, provider permissions, workspace data
behavior and distributed quotas were not exercised. Longer cursor cycles stop
at the page cap; cross-context cursor misuse is documented caller responsibility.

## Final artifact sizes

Counts include comments and blank lines and count a final unterminated fixture
line (`truncated.json`) as one logical line.

| Area | Files | Logical lines |
| --- | ---: | ---: |
| src | 4 | 503 |
| tests, including shared support | 3 | 730 |
| fixtures | 15 | 542 |

Source breakdown: lib.rs 119, models.rs 133, error.rs 202, pages.rs 49.
Test breakdown: protocol.rs 470, workflows.rs 233, support/mod.rs 27.
The 15 fixtures include the independent 37-case error matrix. Runtime-generated
oversized and truncated-wire specimens are counted as test code, not stored
fixture lines. Documentation and manifest lines are excluded from this table.

## Resource observations

Before building: `df -h .` showed 207 GiB available on the 915 GiB workspace
filesystem; `free -h` showed 30 GiB RAM, 24 GiB available; `nproc` showed 20.
The worktree had no existing target output when inspected. The initial canary
used the user's required four Cargo jobs, one Cargo process. After that build,
`du -sh target` showed 606 MiB; after tests 789 MiB; after Clippy/docs about
1.1 GiB. Final observation: 203 GiB disk available and 22 GiB RAM available.
These are point samples, not measured per-process peak memory. No cache cleanup
was required and no source, evidence or shared cache was removed.

Owned cache: this worktree's default `target/`, reconstructible build/docs output,
approximately 1.1 GiB. Source and essential evidence live in examples/intercom-sdk
and Cargo.toml/Cargo.lock and are preserved independently of that cache.
