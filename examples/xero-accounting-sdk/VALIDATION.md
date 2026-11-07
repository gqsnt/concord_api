# Validation receipt — 2026-11-07

Working tree based on frozen commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. Toolchain: rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), cargo 1.99.1 (`5f94df478`, 2026-08-27). Source is left uncommitted as requested. Live Xero calls: **NOT_RUN**.

## Required checks

All Cargo builds/checks were sequential with `CARGO_BUILD_JOBS=4`, using the default worktree `target/`; no CARGO_TARGET_DIR override was set. Cached dependencies were available offline. No shared crate was changed to obtain a pass.

| Command | Observed result |
|---|---|
| `CARGO_BUILD_JOBS=4 cargo test --offline -p concord-sdk-xero-accounting --no-run` | PASS, first unlocked build; added this package's lock stanza, no downloads/new third-party packages. Initial dependency build completed in 15.38s. |
| `CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-xero-accounting` | PASS: 17 protocol tests, 8 workflow tests, 1 compiled README doctest; no ignored tests. |
| `CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-xero-accounting --all-targets -- -D warnings` | PASS, final checked source including the synchronized refresh flag and retry lease handling. |
| `CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-xero-accounting --no-deps` | PASS; generated `target/doc/concord_sdk_xero_accounting/index.html`. |
| `rustfmt --edition 2024 --check examples/xero-accounting-sdk/src/*.rs examples/xero-accounting-sdk/tests/*.rs examples/xero-accounting-sdk/tests/support/mod.rs` | PASS. Scoped to owned files; workspace-wide formatting was not run. |
| `git diff --check` | PASS. |
| Parsed baseline/current Cargo manifests and locks with Python tomllib | PASS: only the requested workspace member differs; the sole added lock package is concord-sdk-xero-accounting 0.0.0; no removed or changed existing package entries. |

Earlier diagnostics were retained as observable actions: the first test run had an unused test import and missing test-crate docs, corrected; the first Clippy run refused a large Acquisition enum, corrected by boxing Ticket; the new retained-prefix test initially guessed `Incomplete.items`, and rustc E0609 identified `collected`, corrected. Tests were rerun after expanded coverage, README doctest and the refresh-flag synchronization/retry-lease changes. These earlier failures are not represented as passes.

## Evidence covered

- Both required connected workflows use `TestServer::start(|request: &Recorded| -> Reply)` and assert exact method, route/query, tenant, authorization and media behavior. Creation is independently compared against `fixtures/create-request.json`.
- A real post-exchange rename failure retains **both new tokens** in a durable pending record; no Accounting request uses them before activation. A restart recovers the pair without another exchange.
- Concurrent waiters share refresh; cancellation of the starter preserves the owned flight. Independent sessions are excluded by the durable journal lock. A delayed old 401 cannot remove the new generation.
- A lost token response and crash-held lock fail closed; the previous refresh token cannot silently be replayed after restart.
- Cross-tenant credentials/journal opens, invoice/contact/draft references and page continuations are refused before wire I/O.
- Every documented Accounting error class, unknown status fallback, 2xx validation error, malformed/type-invalid/truncated/oversized body, XML/missing/repeated media, redirects and unsafe write replay are exercised.
- 429 tests cover one bounded safe read repeat, attempt exhaustion, malformed/duplicate/missing hints, a hint exceeding the deadline and shared cooldown refusal. No transport/decode/5xx/401 retry loop exists.
- Modern metadata, legacy short pages/empty exhaustion, bounded continuation, mismatched context, inconsistent metadata/ID and retained failed-page prefixes are covered.
- Ordinary public Debug/Display, nested provider diagnostics and sentinel-bearing bodies withhold tokens, client secrets, resource IDs and private message text.

## Ownership and size

- Source: 6 Rust files, **1520 lines** (physical lines including docs/blanks).
- Tests: 3 Rust files, **1159 lines**, including shared support.
- Fixtures: 13 files, **44 lines**; all synthetic and individually attributed in PROVENANCE.
- Public/docs: README.md, PROVENANCE.md, FRICTION.md and this receipt. Manifest and two allowed root-file edits accompany them.
- No files under shared `crates/`, `checks/`, other examples or scripts were edited. No commit was attempted.

## Resources and limits of evidence

Before building, `df -h .` reported the `/dev/nvme0n1p2` filesystem with about 437 GiB free; `free -h` showed 30 GiB RAM, about 25 GiB available. A later observation reported 20 CPUs, about 434 GiB filesystem free and a 1.1 GiB owned default target. That is an observed retained size, not a measured peak; other work can affect shared disk use. No cleanup was needed. This is a bounded package gate, not a full-workspace or cross-platform gate.

Fixtures establish local HTTP/1.1 behavior, including deliberate body failures, and real local filesystem failure handling. They do not establish live Xero acceptance, production authorization, TLS, network-filesystem durability, Windows crash semantics or end-to-end atomicity across remote token issuance and local disk. The remote/local gap is explicitly handled by fail-closed reauthorization rather than a claim of distributed atomic commit.
