# Phase 13 Conformance Report

## Verdict

`PHASE_13_COMPLETE_WITH_LIMITATIONS`

Phase 13 — Lexically Scoped Rate Limits is complete. The sole limitation is the reserved Phase 14
product-completion, conformance-closure, and release-final work. The normative package remains
`Normative Implementation Candidate`; only Phase 14 may restore `Normative Release Final`.

Phase 14 is active with status `NOT_STARTED`. No Phase 14 product or release-final behavior was
implemented, and no Phase 15 work was started.

## Identities and Git boundary

- Starting HEAD: `0ef7d3f619155b251509da1d6112620b14f5c7b7`
- Authority commits: `c819da2` (`docs: establish lexical rate-limit contract`) and `8df99c2`
  (`docs: align phase 13 contract marker prose`)
- Source/test commits: `25016f44114b65aa922de109bce57278da269990`, `9f102aa4fcfa4c1ec3e881677b78426a8ccf3c74`, `1e8bfe1124638535ca6641b8a890d0139f698093`, `66423ac753bc5394f0f39b62779d590386a95ffe`, `24d427d6e3ccb2df0e7310dd0f59123195ddf050`
- Source-correction commits: `6a43e3c24ddfaff5835e19bc0275d1ab62f664ae`, `3d9da62f4eb13a7e8e4af6de984f4cef9a8cdf10`, `f38ac7b0a47a9a855fa77e16464955d6a144eb95`, `adbca38ad1df407fc55e1fe0dbb08ee319070e30`, `dedf999e204cc7e580a8a0d71392291de9518848`, `74519a183f953b50d18d3f7094b4bbbf2066cb7f`, `ea370cfb07384f399cfd6709f669d5078c6474ab`, `f7ddcec1b898f385b1d00eaf0514ac9505939ed7`, and `3e942f299426aa76fbeca6887c26db89d7509363`.
- Final immutable tested source: `3e942f299426aa76fbeca6887c26db89d7509363`
- Evidence/report/state commit: `EVIDENCE_COMMIT_PENDING`
- Evidence anchor: `ANCHOR_COMMIT_PENDING`
- Metadata descendant: recorded in the controller after the compatible two-file descendant
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

The generated contract transition is exactly `13 → 14`. No generated rate-limit `Display` bound is
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
extension, feature matrix, and MSRV. The maintained all-feature inventory selected 596 tests and
all passed; default Nextest selected 571 tests and all passed.

The Phase 13 evidence catalog contains 46 direct passing Phase 13 rows: 23 retained-version rows
and 23 new-requirement rows, all verified against `3e942f299426aa76fbeca6887c26db89d7509363`.
There are no Phase 13-owned partial rows and no waivers. The catalog has 1,726 total rows after
the Phase 13 additions; coverage has 581 active requirements and 16 superseded historical rows.

Security evidence used distinct sentinels for client, scope, endpoint, pagination, host, URL
query, explicit advanced key parts, credentials, request body, and response body. Normal generated
plans and diagnostics contain only declaration IDs and static quota metadata; advanced key values
remain available only to the intentionally selected custom limiter and remain absent from
automatic diagnostics, hooks, completion, `Display`, `Debug`, sources, and Rustdoc.

## Closure boundary

Every Phase 0–12 evidence row remained byte-identical, every Phase 0–12 report remained
byte-identical, and no historical evidence was rewritten. Phase 14 remains active and `NOT_STARTED`.
Release-final promotion remains reserved exclusively for Phase 14.
