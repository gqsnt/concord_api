# Adoption lane (between waves 2 and 3): new Concord facts in the five wave-2 SDKs, and an opt-in repeated-continuation guard in `concord-pagination`

You work in a git worktree of the Concord Rust workspace (branch `sdk50/b23-w2adopt`). Read
`AGENTS.md`/`CLAUDE.md` if present, `docs/AUTHOR_GUIDE.md` §5.11 ("One header field, strictly"),
§5.11b (`Error::delivery()`), §5.13 ("Name the result": `send P -> Result<_, Error>;`), and
`crates/concord-pagination/README.md`. You cannot commit; leave changes in the working tree.
Network is for loopback tests only; contact no external service.

## Part 1 — `concord-pagination`: opt-in refusal of a repeated continuation

Witnesses (read them): `examples/mailgun-sdk` (a per-traversal `HashSet` of followed URLs),
`examples/zendesk-support-sdk/src/pagination.rs` (repeated cursor refused), `examples/intercom-sdk/src/pages.rs`
(empty/immediately repeated cursor refused, cap required), `examples/hubspot-crm-sdk/src/lib.rs`
(unchanged `paging.next.after` refused), `examples/notion-sdk` (cursor cycle detection). Design the
smallest opt-in on `Pages` (and wherever the same traversal applies) that refuses a continuation
key already used in this traversal, with a typed, value-free error that keeps the collected prefix
and progress like the crate's other failures; state its memory cost (one key per page) and how it
interacts with `max_pages` and `Resume`. Defaults unchanged. Tests: immediate repeat, longer cycle,
resume after a refusal, cap interplay, no `Clone`/`Hash` cost for callers who do not opt in.

## Part 2 — adopt in `examples/{postmark,mailgun,zendesk-support,intercom,hubspot-crm}-sdk`

For each SDK, only where it fits and **removes or simplifies** code (measure lines before/after):
1. `send Policy -> Result<_, Error>;` (or the SDK's actual uniform output shape) on the client, so
   generated `send()` rustdoc names the concrete result. Skip where the output is not uniform.
2. `concord::field::{single, unsigned}` instead of hand-written strict header parsing (Intercom
   `src/error.rs::number`, Postmark `Retry-After` via `trim().parse()`, others you find).
3. `Error::delivery()` where the policy classifies a failed write: replace `transport().is_connect()`
   checks and method-only heuristics. **Behavior rule:** Postmark, HubSpot and Mailgun currently
   classify a connect refusal of a write as uncertain. If adopting `delivery()` would change an
   outcome that an existing test asserts, do NOT edit that assertion: keep the old behavior in that
   SDK and report the conflict precisely (test name, old vs new outcome). Where no test pins it,
   you may make `NotSent` non-uncertain and add a test for it.
4. The new pagination guard where an SDK hand-writes the same check.

Rules: every existing test assertion stays unchanged; README/rustdoc updated where behavior or
public signatures change; no other crate edited except `crates/concord-pagination`.

Verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time): `cargo test -p concord-pagination`
(default and `--features link`), `cargo test -p <each changed SDK>`, clippy `--all-targets
-D warnings` for those packages, rustdoc `-D warnings`, `cargo fmt --all -- --check`.

## Report

Part 1: public API added (one line each with justification), tests. Part 2: per SDK and per item,
lines removed/added (production vs tests/docs), what was skipped and why, any behavior conflict
(item 3). Then commands and results.
