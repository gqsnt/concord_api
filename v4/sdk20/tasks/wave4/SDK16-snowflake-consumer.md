# SDK16 Snowflake SQL API — independent first-use consumer

Requested runner: Claude Opus subagent. You did not write this SDK and have not seen its author's notes.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `0983a349`; its `HEAD` may
  also contain this SDK's own earlier role commits (author, consumer), which is expected. Prefix
  every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b` (a shared build cache; if cargo says it is
  waiting for a lock, wait).
- **Frozen library:** do not edit `crates/`, `scripts/`, `docs/`, `checks/` or other examples.
  If Concord forces a workaround (private reach-through, rebuilt execution, handwritten operation
  names, a native escape you think should not be needed), keep the working code and record it as
  a friction entry with a reproduction; do not patch the library.
- Use Concord's public API (`concord`, `concord-pagination`, `concord-extras` through the facade)
  and its docs: `README.md`, `docs/*.md`, crate rustdoc (`cargo doc -p concord --no-deps
  --features csv,jsonl`), `crates/*/README.md`. The five existing provider SDKs under `examples/`
  are normal reading material (say so if one supplied your solution).
- No live provider calls, no credentials, no secrets: all auth values are synthetic and must not
  look like real credentials. Tests run offline against the loopback server in
  `examples/testserver` (a dev-dependency by path).
- Fixture bytes are hand-authored from the official documentation unless you actually obtained a
  documented example; record provenance honestly. Put byte-sensitive fixtures under
  `examples/snowflake-sdk/fixtures/` and keep `examples/snowflake-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.

## Your situation

You are an application developer adopting `concord-sdk-snowflake` (in `examples/snowflake-sdk/`). Use
its `README.md` and rustdoc (`cargo doc -p concord-sdk-snowflake --no-deps`) and Concord's public
docs. Reading the SDK's `src/` or its tests counts as an intervention: record when and why. Do not
read `FRICTION.md`. Do not modify the SDK's `src/`, `README.md` or existing tests.

## Business outcomes

- **A.** Submit async statement, reuse first result and discover then fan out remaining partitions.
- **B.** Submit and explicitly cancel a separate long-running statement.

Provider facts you may rely on (from the official documentation): see the sources below and the
SDK's README. Sources: - https://docs.snowflake.com/en/developer-guide/sql-api/submitting-requests
- https://docs.snowflake.com/en/developer-guide/sql-api/handling-responses
- https://docs.snowflake.com/en/developer-guide/sql-api/cancelling-requests

## What to build

`examples/snowflake-sdk/tests/first_use.rs`, starting from an empty file: an application-style test
suite that reaches both outcomes through the SDK's public API against a local fake of the provider
that **you** write from the documentation (use `concord_example_testserver`; you may reuse the
SDK's documented fixture files only if the README points to them). Check the wire requests you
expect, the results, and the meaningful failure/completion cases (a failed page, a provider error
document, a stream that ends early, …) — whatever the outcome makes relevant.

Before writing each outcome, write your prediction (request, return value, how completion and
failure are signalled) into `examples/snowflake-sdk/FIRST_USE.md`; then keep a short ordered trace
there: first entry point, docs used, distinct compiler diagnostics, interventions (source reads,
native escapes), the working path, and concrete friction. No numeric scores.

Command: `cargo test --locked -p concord-sdk-snowflake --test first_use`.
Final message: test count passing, the first intervention (if any), and the top friction points.
