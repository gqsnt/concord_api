Original source SHA: b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e
Independent snapshot HEAD: 5807d4b7ea4d39c97a8f2a2370fa4f61c16b00f9
Review mode: execute; requested model: gpt-6.1-sol; requested effort: xhigh.
Verify effective settings; report personally executed versus inspected-only evidence.
Never push, publish, modify global settings, bypass sandbox, or claim acceptance before triage.

## Supplied document: FOCUSED.md

# Focused re-review of the successor candidate

You are the independent reviewer (local Codex GPT-6.1 Sol, xhigh); verify the effective settings.
The previous final acceptance review of `0fe60e155673a2b7b794d4ce389b6609d38815cb` (archived in this
snapshot at `v4/refinement/reviews/codex/final_0fe60e1_report.md`) returned REVISE with three
findings: R1 (P2, generated per-call default wording for an initialized optional override), R2 (P2,
the current claim-to-test matrix `v4/TEST_MATRIX.md`), R3 (P3, stale diagnostic prose in
`docs/MIGRATION.md` and `docs/DIAGNOSTICS.md`).

This snapshot is the successor `b0fe26e4ec3d0c2ac7e1ebb4216d05a38a4aff4e` (see
`.concord-review.json`). The only changes since `0fe60e1` are the coordinator's corrections for R1–R3
(macro doc-note wording plus a unit test in `crates/concord-macros/src/emit.rs`, the matrix, the two
prose files) and the archived review files under `v4/refinement/reviews/codex/`.

Do, in this order:
1. Verify source identity and settings. Build fresh (`--locked`, offline) and run
   `CARGO_NET_OFFLINE=true scripts/verify.sh --msrv` yourself.
2. For each of R1, R2, R3: reproduce the original failure against the evidence you can derive (for
   R1 your own declaration with an initialized optional `each ... else inherit` value; inspect the
   generated rustdoc text), then decide RESOLVED / NOT RESOLVED with evidence. For R2, check every
   PASS row of `v4/TEST_MATRIX.md` against an existing assertion (sample generously; report any
   cited test that does not exist or does not assert the claim). Check that the new generated-doc
   test actually discriminates (e.g. revert the wording in a disposable copy under `target/probes/`).
3. Look for regressions introduced by the delta, and for any remaining false claim in the
   user-facing docs that contradicts executed behaviour (spot-check, prioritise defaults, presence,
   overrides, representations, errors and redaction).
4. Give a scoped verdict for `b0fe26e`: ACCEPT (no blocker/high, previous findings resolved) or
   REVISE/BLOCKED with reasons. State platform limits honestly (Windows/macOS were not run on this
   candidate by anyone; only Linux evidence exists).

Do not push, publish or change global settings; keep production source unchanged (or report the
exact diff); stop any server you start. Your final message is the report.


## Supplied document: ENV.md

# Execution environment for this run

- Sandbox: workspace-write confined to this snapshot, network access enabled for this run only so
  loopback tests can bind sockets (the Linux workspace sandbox forbids all sockets otherwise). Do not
  contact external services; everything needed is local.
- `CARGO_HOME` is a private registry prefetched with `cargo fetch --locked`, located OUTSIDE the
  snapshot (granted as an extra writable root) so trybuild's path normalization behaves as in an
  ordinary checkout. `CARGO_TARGET_DIR` is `target/review-build`. Build fresh with `--locked`
  (add `--offline` if cargo tries the network). Toolchains: 1.97.1 (rust-toolchain.toml), 1.85
  (`cargo +1.85`), nightly.
- Put probes under `target/probes/`; restore or report any production-source change; the original
  source integrity is checked after the run. Stop any server you start before finishing.


## Supplied document: 01_CHARTER.md

# 01 — Product charter and authority

## Mission

Polish the existing Concord V4 into a desirable Rust API-client authoring and calling library. Continue the current code. The earlier clean-slate V3→V4 instructions are historical and must not trigger deletion or another rewrite.

Priority order: current user instructions; this charter's product boundaries; executable evidence and the newly chosen contract; current V4 implementation/specification as a revisable baseline; V3 as historical knowledge. Conflicts must be recorded, not silently papered over.

## What remains inside

Client/scope/endpoint structure, typed ordinary values and their legitimate storage, deterministic construction rules, path/subdomain/query/header composition, sensitivity-aware Concord diagnostics, minimal body/response conversion, and native reqwest interoperability. A little well-justified convenience is allowed when it eliminates repeated application code without acquiring a workflow responsibility.

## What remains outside

Authentication orchestration, credential discovery/fetch/refresh, retry scheduling, rate admission and pagination traversal. Do not reintroduce them through DSL annotations, hooks, core-owned registries, service containers, sidecar schedulers, default-feature crates, or a generic managed executor.

A caller-provided token is ordinary possibly sensitive data. A cursor or page-size query parameter is ordinary endpoint data. Client-local storage is not automatically an auth store. A caller may supply an `Arc<T>` or an application object; Concord must not invent lifecycle or refresh policies for it. Study genuine storage needs without turning Concord into an application-state container.

Application-only examples may show those four workflows to prove composability. They must not become supported Concord subsystems or a project to build four companion crates.

No new OpenAPI importer, multi-client federation, cache framework, request graph, plugin ecosystem, website application, LSP, or documentation-export engine in this campaign. Native request/response use is permitted even when a convenience surface is deliberately absent.

## Product quality priorities

1. Correctness and honest semantics are constraints, not scores that style can outweigh.
2. Predictable author/caller behavior is more important than minimal token count in the DSL.
3. Prefer a small number of orthogonal concepts and one canonical way to do a task.
4. Preserve native compatibility; do not wrap every reqwest option in Concord vocabulary.
5. A small implementation that shifts complexity onto every user is not automatically better.
6. An additional helper/type is acceptable when a real task and tests show a net reduction in complexity.
7. Prefer ordinary Rust when it is sufficient; do not force awkward Rust solely to keep a macro primitive count low.

## Compatibility and permissions

The inspected workspace is an unpublished V4 alpha. Local source compatibility is not sacred: justified changes to its syntax or generated API are authorized. Record a migration note and update all maintained callers. Do not retain a second legacy parser just to preserve the alpha. Inspect local downstream uses before choosing a break.

Local branches/worktrees, commits, tests, docs, benchmark artifacts and scoped specialist edits are authorized. Preserve V3, existing branches, user changes, agent configuration and unrelated files. No force reset, blanket clean, recursive deletion of user paths, publication, push, release, global model setting changes, package/account changes, or paid cloud operations are authorized by this handoff.

Normal design choices do not require another approval round. Pause only for destructive actions, essential unavailable access, or a genuine conflict with the product boundary. Continue independent local work where a platform gate is blocked; report the gate accurately.

## Practical definition of desirability

The standard task should be shorter and more understandable than a good handwritten reqwest wrapper. The unusual task should have a documented, typed exit. A wrong declaration should fail near the cause. A maintainer should be able to trace one endpoint from syntax to request without learning an orchestration system.

Use qualitative evidence and executable tasks. Do not manufacture an aggregate desirability score, infer user preference from line counts, or call agent simulations a human usability study.
