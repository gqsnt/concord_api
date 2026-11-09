# Concord adaptation costs

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Baseline remains frozen; **no shared crate changes were made**. This is an
**example-assisted** result. Evidence below consists of source/API contracts read, code written
in this package and fixture observations, not private reasoning transcripts.

1. **Sentry Link semantics still need an adapter.** `concord-pagination/link::parse` handles
   field grammar/URL resolution and `relation` detects ambiguity, but does not choose results,
   compare cursor parameter versus URL, enforce a collection/filter, detect non-progress or
   require a page bound. `src/pagination.rs` writes these Sentry rules and the two small
   generated-operation-to-Page adapters. `protocol.rs` refuses malformed/ambiguous/foreign/
   contradictory continuations; both connected workflow tests stop on the supplied false flag.
   These are provider semantics and belong here. Generic cost: Parameter exposes only
   `raw_value`, so this SDK also unquotes/unescapes it by hand. Smallest useful shared change:
   a checked decoded-value accessor on `link::Parameter`; retain all continuation policy in the SDK.

2. **Quota interpretation remains provider code.** The rate-limit README explicitly leaves
   header names and observations to SDKs; `Observation::Window` takes a monotonic deadline,
   whereas Sentry supplies UTC seconds. `src/rate.rs` uses shared strict field readers, checks
   relationships, converts epoch/reset units and keeps concurrent remaining separate from rate
   remaining. The concurrency fixture leaves 99 frequency requests but zero concurrent slots;
   scheduling tests prove retry waits for the owned lease. The provider does not give remote
   request lifetimes, so ConcurrentRemaining cannot become a correct cross-process lease.
   Smallest generic improvement: a checked helper converting an injected wall-clock deadline
   to a limiter deadline. No shared change should invent Sentry field meanings or external leases.

3. **Composing retry and admission takes explicit lifecycle code.** `Retry::receive`'s sync hook
   cannot await either admission. `receive_with` **does fit**: its async hook owns a Lease,
   awaits frequency admission, dispatches once, preserves final context during decoding and
   drops context before backoff. `src/policy.rs` wires this to a generated SendWith policy;
   it also classifies only this slice's safe read outcomes and observes provider cooldowns.
   `retry_reacquires_concurrency_and_releases_lease_during_backoff` holds a competing clone's
   request open beyond backoff, then observes exactly one retry after release. The engine
   already expresses count/deadline/replay/concurrency composition; there is **no missing
   concurrency mechanism**. Smallest improvement: a compiled cross-crate SendWith example using
   `receive_with`, limiter lease/acquire and final body decode, reducing assembly work.

4. **Final body deadline is a separate boundary.** Retry rustdoc ends scheduling at the final
   head. A finite decode and error-document read occur afterwards, while the SDK still owns
   the lease. The package supplies a transport request timeout plus declared/body-document
   byte limits, and documents that the retry deadline does not include final consumption.
   Stalled-body and mid-body-disconnect fixtures verify no early lease release or body replay;
   a second request times out during admission with zero dispatches. Smallest optional shared
   improvement: a runtime-neutral final-consumption deadline adapter preserving operation/head
   diagnostics; retry itself should continue to avoid replaying partially consumed bodies.

5. **A scope's own resource read needs a trailing slash.** AUTHOR_GUIDE §3 permits trailing
   slashes only on endpoint paths. To retain Sentry's exact `/issues/{id}/` wire spelling,
   `scope issue` declares `path "issues"`, and its three endpoints repeat `{issue.issue}`
   with their final slash. The four-wire-shapes test checks these literal paths. This is a
   declaration ergonomics cost, not provider model logic. Smallest possible improvement:
   an explicit endpoint suffix representing the current scope plus a trailing slash, without
   changing absolute-path or ordinary segment semantics. No native URL mutation was necessary.

6. **Provider errors and safe model formatting are SDK work.** Concord provides strict bounded
   JSON/media checking, preserved heads and redacted transport/codec errors. It cannot choose
   the provider error schema/status classes or decide which Issue/Event content is sensitive.
   `src/error.rs` supplies classifications, boxed payloads and explicit document access;
   `src/model.rs` withholds models/values/cursors in Debug. Protocol tests exercise every
   documented class and both status paths for malformed/truncated/oversized/wrong-media bodies.
   The Frankfurter example shortened discovery of this pattern (see below). No core feature is
   missing; a reusable documentation recipe for safe model Debug/error wrappers is the smallest
   improvement. The shared JSON codec fits without a custom codec or relaxed media policy.

7. **Keyed limiter queues can delay unrelated endpoints.** `concord-ratelimit` README/rustdoc
   explicitly defines a shared FIFO lease queue and a separate shared FIFO rate queue across
   keys; a blocked head may delay a different key. This SDK uses one limiter with four endpoint
   keys to share one caller's bounded state, accepting conservative head-of-line waiting.
   It does not claim maximum parallel throughput. The provider scopes limits per caller/endpoint;
   the queue's cross-key fairness is generic policy. Smallest optional improvement: per-key FIFO
   for single-key leases. SDK alternative: four separate limiters, with additional routing/state
   assembly. No frozen shared behavior was altered.

## Shared capability decisions and exact boundaries

| Crate/feature | Used for | What it does not express here |
| --- | --- | --- |
| `concord` / `api!`, Endpoint, SendWith | Four declared GETs; client→scope→endpoint navigation; sensitive bearer placement; ordinary generated send; response metadata/errors; declaration limit | Sentry model vocabulary, HTTP retry eligibility or header meanings. |
| `concord-codecs/json` via `concord/json` | Strict typed success/error JSON, media checks and safe codec diagnostics | Sentry Problem/Issue/Event schemas. No extra codec feature needed. |
| `concord-auth` | Validated, sensitive Bearer HeaderValue | Caller identity/token-to-quota association. Cache/refresh intentionally unused for a supplied static token. |
| `concord-pagination/link` + Pages | Complete Link grammar and bounded dependent traversals with prefix/resume errors | results flag, cursor agreement, provider collection/filter trust, immediate cycle refusal; implemented in SDK. |
| `concord-retry/concord` | receive_with, replay preflight, attempt/deadline/backoff accounting, final context retention; shared Retry-After parser | Sentry retryable statuses and quota observations; final body consumption. Async admission and concurrency lease lifetime **are** expressible. |
| `concord-ratelimit/tokio` | Local fixed windows, learned frequency windows/cooldowns, concurrency leases and cancellation | Provider header decoding, absolute wall-clock conversion, caller identity, distributed concurrency; implemented/documented in SDK where possible. |

## Example-assisted disclosure

- `examples/frankfurter-sdk/src/lib.rs`: borrowed the overall bounded error-document /
  decode-with-meta SendWith pattern, not provider-specific fields or test expectations.
- `examples/testserver/src/lib.rs`: consulted the required fixture server API, native request
  recording and lifecycle/gate/body tools; used the unmodified harness.
- `examples/testserver/Cargo.toml`: consulted the package name and test feature wiring.

These are all example files directly read. Required guides also contain embedded example
snippets; no other SDK example files were opened. See `PROVENANCE.md` for the full source
inventory, fixture table, official page URLs/dates and command record.
