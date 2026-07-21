# Phase 13 Conformance Report

## Verdict

`PHASE_13_COMPLETE_WITH_LIMITATIONS`

Phase 13 — Lexically Scoped Rate Limits is complete. The sole limitation is the reserved Phase 14
product-completion, conformance-closure, and release-final work. The normative package remains
`Normative Implementation Candidate`; only Phase 14 may restore `Normative Release Final`.

Phase 14 is active with status `NOT_STARTED`. No Phase 14 product or release-final behavior was
implemented, and no Phase 15 work was started.

## Identities and Git boundary

- Starting HEAD: `8932f66f3be433f1b4e05752ee86ae53b73e2b89`
- Authority commits: `c819da2` (`docs: establish lexical rate-limit contract`) and `8df99c2`
  (`docs: align phase 13 contract marker prose`)
- Source/test commits: `25016f44114b65aa922de109bce57278da269990`, `9f102aa4fcfa4c1ec3e881677b78426a8ccf3c74`, `1e8bfe1124638535ca6641b8a890d0139f698093`, `66423ac753bc5394f0f39b62779d590386a95ffe`, `24d427d6e3ccb2df0e7310dd0f59123195ddf050`
- Source-correction commits: `6a43e3c24ddfaff5835e19bc0275d1ab62f664ae`, `3d9da62f4eb13a7e8e4af6de984f4cef9a8cdf10`, `f38ac7b0a47a9a855fa77e16464955d6a144eb95`, `adbca38ad1df407fc55e1fe0dbb08ee319070e30`, `dedf999e204cc7e580a8a0d71392291de9518848`, `74519a183f953b50d18d3f7094b4bbbf2066cb7f`, `ea370cfb07384f399cfd6709f669d5078c6474ab`, `f7ddcec1b898f385b1d00eaf0514ac9505939ed7`, `3e942f299426aa76fbeca6887c26db89d7509363`, `8a5bfee7c227aad4be4ee5d5d898da033a4fa10b`, `d7f418b0792ec05e747fe9c3e7c533701617d47c`, and `684789b097b0cf225e880142874ae7eb0136286c`.
- Superseded correction source: `684789b097b0cf225e880142874ae7eb0136286c` (endpoint-module-dependent inherited IDs and incomplete evidence sequencing).
- Superseded source/test state: `15b61d57c668fead307eafc7a276019760ae46f7`.
- Final immutable tested source: `d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304`
- Superseded evidence/report/state: `7158894bf636803014999f38635a002bfe7e7728`; superseded anchor: `0602840f942cb62a0f9f29661c6d42edf03960f5`.
- Superseded evidence/report/state: `5feb3fabe6372a9b08c595caf8b6199c127d5181`; superseded anchor: `bc3faae853083ef50ba445da70f90abc864dffb1`.
- Evidence/report/state commit: `847679cff263ce642227bd0a042456a20495416b`
- Evidence anchor: `ANCHOR_COMMIT_PENDING`
- Metadata descendant: compatible documentation-only descendant recorded after the corrected anchor
- Additional executable correction commits after the superseded source include the retained-source validation chain through `3ff65c60dc4f360663f13d0e122fa102f6d21999`, followed by `1dabc1b`, `40cc4e7`, `14c0d18`, `23b1b32`, `6dd7791`, `47bee70`, `116c5dc`, and `d0891ac`.
- Branch: `master`; final tree is required to be clean
- Head policy: `anchor-must-be-an-ancestor`

No remote Git operation, branch operation, history rewrite, publish, deploy, credential access, or
controller-policy edit occurred.

## Authority and requirement lineage

`docs/normative/11_CONCORD_REQUIREMENT_CHANGE_RECORD_0.4.md` is registered as the Phase 13
lineage owner. It records every retained, superseded, and new requirement, prior title and digest,
normative owner, authorizing directive, implementation phase, and historical-evidence effect.

Retained with explicit version change:

`PROD-GUAR-010`, `PROD-STATE-001`, `DSL-RATE-001`, `DSL-RATE-002`, `API-RATE-004`,
`GEN-BUILD-004`, `GEN-DOC-001`, `GEN-DOC-002`, `SEC-RATE-001`, `SEC-RATE-002`,
`BOUND-RATE-005`, `EXEC-RATE-001`, `EXEC-RATE-002`, `EXEC-RATE-003`, `FEAT-GOV-001`,
`FEAT-GOV-002`, `CONF-COMPILE-001`, `CONF-COMPILE-002`, `CONF-COMPILE-003`,
`CONF-DOWNSTREAM-002`, `CONF-SEC-001`, `CONF-MSRV-001`, `CONF-DOC-001`.

Superseded mappings:

`DSL-RATE-003 → DSL-RATE-011`, `DSL-RATE-004 → DSL-RATE-009`,
`DSL-RATE-005 → DSL-RATE-010`, `DSL-RATE-006 → DSL-RATE-010`,
`DSL-RATE-007 → DSL-RATE-011`.

New Phase 13 requirements:

`PROD-MODEL-009`, `PROD-GUAR-015`, `PROD-NONGOAL-003`; `DSL-RATE-008` through `DSL-RATE-012`;
`API-RATE-005`; `GEN-RATE-001` through `GEN-RATE-006`; `SEC-RATE-003`;
`EXEC-RATE-004`, `EXEC-RATE-005`; `COMPAT-MACRO-006`; and `CONF-RATE-001` through
`CONF-RATE-004`.

`ADR-046 — Generated rate limits use lexical ownership and global keys` is accepted and preserves
ADR-014, ADR-017, ADR-018, ADR-030, and ADR-035. The Phase 0–12 evidence catalog and reports are
byte-identical at their historical owners; old rate-limit evidence remains resolvable only against
its historical paragraph and source.

## Implemented contract

The approved direct grammar is:

```text
rate_limit_decl =
    "rate_limit" "{"
        cost_clause?
        rate_window+
    "}" ;

rate_limit_observer_decl =
    "rate_limit_observer" rust_path ";" ;

cost_clause = "cost" positive_integer ";" ;
rate_window = positive_integer "/" duration_literal ";" ;
```

Client, scope, and endpoint layers accept at most one complete direct declaration. Cost defaults to
one; windows and cost are positive, static, bounded, and ordered. The observer is optional,
unique, and client-only, and is independent of whether the client owns a quota.

The old normal model is removed: `rate_limits { ... }`, named templates, named attachments,
`by [...]`, `rate_limit replace`, `rate_limit off`, direct named rules, `observe PATH;`, dynamic
dimensions, generated key-part formatting, and rate-limit-only `Display` bounds are rejected with
focused migration diagnostics where applicable.

Effective plans are inherited in exact order: client, outer scope to inner scope, endpoint. Child
rules add to ancestors and cannot replace, disable, or modify them. Scope rules cover complete
lexical subtrees; endpoint rules cover only that endpoint. No discontiguous normal generated quota
grouping or request-value partitioning is modeled.

Each declaration receives a deterministic identity derived only from `module_path!()`, canonical
client name, complete lexical scope path, endpoint name when applicable, and declaration kind.
Every normal generated rule uses `RateLimitKey::global()` with static cost and windows. Generated
plans contain no `RateLimitKeyPart` and no host, method, scope value, endpoint value, pagination
continuation, body, credential, or formatted request value.

The generated contract transition exactly `13 → 14`. No generated rate-limit `Display` bound is
collected; independent host/path/query/header formatting bounds remain intact. Generated Rustdoc
reports empty/nonempty effective plans, lexical origins, costs, windows, and composition order,
without old syntax or runtime values. It identifies declaration-scoped global buckets and the
advanced prepared/custom boundary.

The observer configures only the generated default governor. Explicit limiters own their own
observation; clones share selected limiter state; independent generated builds receive independent
default governor state. Advanced prepared/custom users retain `RateLimitId`, `RateLimitKeyPart`,
`RateLimitKey`, `RateLimitWindow`, `RateLimitRule`, `RateLimitPlan`, `RateLimiter`,
`GovernorRateLimiter`, `NoopRateLimiter`, explicit bounded keys, call-order composition, redacted
inspection, and pre-send invalid-key failure.

The proven limiter lifecycle was preserved: hook veto precedes provider/limiter work; credential
preflight precedes limiter waiting and finalization follows it; reservations are atomic and not
consumed while waiting; cancellation and recovery behavior remain bounded; hidden native retries do
not reacquire; response observation precedes body polling and cannot replace the response; cooldown,
finite-state, fail-closed, custom-limiter, no-op, and redaction guarantees remain intact.

## Validation and evidence

The final source passed the complete maintained `just release` matrix on Rust/Cargo/Rustdoc 1.85.0,
cargo-nextest 0.9.140, cargo-deny 0.20.2, just 1.46.0, PowerShell 7.6.3, and Git 2.45.1. The
exact command/result record is `git_diff/phase-13-validation.md`; the cumulative source patch is
`git_diff/phase-13-cumulative-source.patch`.

Focused proof covered parser passes/fails and migration UI, lexical normalization, generated global
bucket semantics, deterministic identity under shared limiters, no-`Display` compilation,
advanced explicit keys, no-governor fail-closed behavior, observer ownership, governor lifecycle,
pagination plan reuse, security sentinels, generated/public Rustdoc, examples, downstream
extension, feature matrix, and MSRV. The maintained all-feature inventory selected 606 tests and
all passed; default Nextest selected 581 tests and all passed. The source-matched validation registry
contains 231 rows, including 52 Phase 13 rows and every maintained matrix command at the final source.
The locked/offline standalone fixture selected exactly one test and passed; its sentinel recorded
`generated=4 prepared=4 fallback_sends=0`.

The Phase 13 evidence catalog contains 46 direct and 0 partial Phase 13 rows: 23 retained-version rows
and 23 new-requirement rows, all verified against `40cc4e7ff8869440248fbfea00f4b6c4fc0e6968`.
There are no Phase 13-owned partial rows and no waivers. The catalog has 1,726 total rows after
the Phase 13 additions; coverage has 581 active requirements and 16 superseded historical rows.

Security evidence used distinct sentinels for client, scope, endpoint, pagination, host, URL
query, explicit advanced key parts, credentials, request body, and response body. Normal generated
plans and diagnostics contain only declaration IDs and static quota metadata; advanced key values
remain available only to the intentionally selected custom limiter and remain absent from
automatic diagnostics, hooks, completion, `Display`, `Debug`, sources, and Rustdoc.

## Correction proof

The corrected generated implementation evaluates `module_path!()` only in private generated-root
constants. Each client, scope, and endpoint declaration receives one root constant for its complete
canonical declaration path; descendant endpoint modules reference that constant. The custom limiter
tests prove one client ID across direct, sibling-scope, and nested endpoints, one outer-scope ID
across immediate and nested descendants, distinct sibling and endpoint IDs, deterministic repeated
build/clone IDs, and collision resistance across generated modules and clients. The governor tests
prove a client quota is shared from a direct endpoint to a descendant, a scope quota is shared from
an immediate endpoint to a nested descendant, and unrelated sibling/outside/endpoint-local rules do
not share buckets.

The standalone `no_governor_rate_limits` fixture proves that an empty generated plan succeeds on
the fallback, a nonempty generated plan fails closed before send, deliberate custom/no-op limiters
succeed, and the DSL observer is not substituted for explicit limiters. It also proves an empty
prepared plan succeeds, a nonempty prepared fallback fails before send with `RateLimit`/
`InvalidConfiguration`, and custom/no-op prepared execution succeeds with global and explicit key
parts captured intentionally. The resolved offline feature graph contains no
`concord_core/rate-limit-governor`, while `NoopRateLimiter` and custom `RateLimiter` remain usable.

The advanced inspection test captures exact IDs, key parts, costs, capacities, durations, and call
order for global, one-part, and multi-part prepared rules. The corrected pagination fixture executes two pages with a changing continuation. Captures show page
indices `[0, 1]`, identical ordered declaration IDs, global keys with zero parts, unchanged static
costs/windows, and the continuation present only in the ordinary request. The real governor test
allows page one and prevents page two from reaching the server before cancellation. Existing core
authentication recovery and cancellation tests, plus the focused final-source lifecycle command,
reprove reacquisition and cancellation behavior without changing the pagination controller contract.

The old source was not executable-only because it already contained Phase 13 evidence/state, and its
inherited IDs were assembled inside endpoint modules. The corrected source sequence is executable-only
commits `369ce1b…`, `738ab67…`, `15b61d5…`, `ceb3d9b…`, `93ec979…`, `dfc15d1…`, `97df170…`, `62809cf…`, `8cd5a60…`, `b739454…`, and `1ecd41d…`, followed only by evidence/state, one anchor, and one
metadata descendant. Every Phase 13 evidence command is now bound verbatim to a passing validation row
for the final source; no Phase 13 row uses only `just release`.

The exact historical Phase 0–12 catalog blob was restored before appending the corrected rows:
1,344,331 bytes, SHA-256
`8300c71829e3ed5f4049d74832bea64a3ed3c46de7b731555de4567a05334b05`, 1,680 rows. A binary prefix
comparison, rather than parsed-row equality, proves its preservation. The corrected catalog has 1,726
rows, 581 active coverage requirements, 16 superseded requirements, and 219 validation records.

## Focused correction closure

The superseded `15b61d5…` state had three remaining classes of defects: lossy private symbol names,
non-positive real-governor wait oracles, and incomplete pagination recovery/cancellation capture.
The final correction uses generated client name plus deterministic declaration ordinal for private
symbols while retaining canonical runtime identity strings. The `A__B` versus nested `A::B` legal
DSL regression compiles and produces distinct IDs.

Real-governor tests now include unrelated transport controls, controlled Tokio time, and successful
post-cancellation acquisitions. Pagination executes multiple pages, captures identical IDs/global
keys/costs/windows, compares recovery reacquisitions exactly, and uses a deterministic later-page
gate proving cancellation prevents later acquisition and send.

The 46-row claim audit is complete in `git_diff/phase-13-evidence-claim-audit.md`; every row has
independent atoms, principal paths, exact commands, and a passing source-matched validation row.
All rows are direct, source-matched to `d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304`, and waiver-free.

## Closure boundary

Every Phase 0–12 evidence row remained byte-identical, every Phase 0–12 report remained
byte-identical, and no historical evidence was rewritten. Phase 14 remains active and `NOT_STARTED`.
Release-final promotion remains reserved exclusively for Phase 14.

## Retained and conformance evidence reconciliation

This correction supersedes the prior evidence state `77f9b2c4fae5c4a63a4898eb4fd23d55c07c0932` and
anchor `826d240d96b74741a543320d377f949abb2fa6b6`. Source `40cc4e7ff8869440248fbfea00f4b6c4fc0e6968`
was replaced because the runtime-snapshot test and retained response/cooldown allocations required
new executable validation inputs. The final immutable executable source is
`d0891ac8de5e44c5da7d8d1202c3a9e4cfa4b304`; its correction commits are `14c0d18`, `23b1b32`,
`6dd7791`, `47bee70`, `116c5dc`, and `d0891ac`. The final two commits correct the tracked
validation-schema field and inherited-source audit binding; the complete maintained matrix was
rerun at `d0891ac`.

The corrected allocations are explicit for `PROD-GUAR-010`, `PROD-STATE-001`, `DSL-RATE-002`,
`GEN-BUILD-004`, `EXEC-RATE-002`, `CONF-COMPILE-001`, `CONF-COMPILE-002`,
`CONF-DOWNSTREAM-002`, and `CONF-RATE-004`. They now bind independent fail-closed atoms,
build-time selections, static cost validation, observer ownership, atomic reservation, public-only
compile and downstream workflows, and the complete limiter lifecycle to focused commands rather
than identity, cancellation-only, UI-only, or aggregate-only evidence.

New source-matched validation records `P13-VAL-053` through `P13-VAL-055` cover the complete runtime
snapshot, response/stream first-excess, cooldown/finite-state, and independent generated-handle
proofs. Together with `P13-VAL-041` through `P13-VAL-052`, they cover the parser boundary,
runtime-state snapshots, fail-closed product cases, governor observer/atomicity cases, public
extension and feature matrices, downstream workflows, lifecycle, pagination, metadata contradiction,
and explicit no-op ownership. Every exact command in the 46-row CSV has a passing validation record
at the final source; no Phase 13 row uses only `just release`.

The completed ignored claim audit has one reviewed section for every Phase 13 evidence row, including
the active owner, paragraph digest, independent atoms, principal paths, exact commands, selected
tests, source-matched validation IDs, and direct conclusion. Final counts are 1,726 evidence rows,
46 direct Phase 13 rows, 597 coverage rows (581 active and 16 superseded), and 234 validation rows
(55 Phase 13). The exact 1,344,331-byte historical prefix remains unchanged with SHA-256
`8300c71829e3ed5f4049d74832bea64a3ed3c46de7b731555de4567a05334b05`.
