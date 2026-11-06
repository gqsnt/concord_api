# SDK19 Microsoft Graph — independent first-use consumer

Requested runner: Claude Opus subagent. You did not write this SDK and have not seen its author's notes.

## Ground rules (all roles)

- Repository: Rust workspace `concord_api`, pinned toolchain 1.99.1, edition 2024. Your working
  directory is a git worktree whose Concord library is frozen at commit `47a25fbf`; its `HEAD` may
  also contain this SDK's own earlier role commits (author, consumer), which is expected. Prefix
  every cargo command
  with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-a` (a shared build cache; if cargo says it is
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
  `examples/microsoft-graph-sdk/fixtures/` and keep `examples/microsoft-graph-sdk/.gitattributes` with
  `fixtures/** -text` so Windows checkouts keep the bytes.
- Record facts, not private reasoning: what you tried, diagnostics, interventions, results.
- **Held-out wave:** this is first-pass validation of a frozen library. Do not read the campaign
  records under `v4/sdk20/`, or any other SDK's `FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md` or
  `INTEGRATION.md`. Other SDKs' source, README and tests remain normal reading material.

## Your situation

You are an application developer adopting `concord-sdk-microsoft-graph` (in `examples/microsoft-graph-sdk/`). Use
its `README.md` and rustdoc (`cargo doc -p concord-sdk-microsoft-graph --no-deps`) and Concord's public
docs. Reading the SDK's `src/` or its tests counts as an intervention: record when and why. Do not
read `FRICTION.md`. Do not modify the SDK's `src/`, `README.md` or existing tests.

## Business outcomes

- **A.** Sync users through delta and reuse final link.
- **B.** Enumerate groups and direct members.

Provider facts you may rely on (from the official documentation): see the sources below and the
SDK's README. Sources: - https://learn.microsoft.com/graph/delta-query-overview
- https://learn.microsoft.com/en-us/graph/delta-query-users
- https://learn.microsoft.com/en-us/graph/api/group-list?view=graph-rest-1.0
- https://learn.microsoft.com/en-us/graph/api/group-list-members?view=graph-rest-1.0

## What to build

`examples/microsoft-graph-sdk/tests/first_use.rs`, starting from an empty file: an application-style test
suite that reaches both outcomes through the SDK's public API against a local fake of the provider
that **you** write from the documentation (use `concord_example_testserver`; you may reuse the
SDK's documented fixture files only if the README points to them). Check the wire requests you
expect, the results, and the meaningful failure/completion cases (a failed page, a provider error
document, a stream that ends early, …) — whatever the outcome makes relevant.

Before writing each outcome, write your prediction (request, return value, how completion and
failure are signalled) into `examples/microsoft-graph-sdk/FIRST_USE.md`; then keep a short ordered trace
there: first entry point, docs used, distinct compiler diagnostics, interventions (source reads,
native escapes), the working path, and concrete friction. No numeric scores.

Command: `cargo test --locked -p concord-sdk-microsoft-graph --test first_use`.
Final message: test count passing, the first intervention (if any), and the top friction points.
