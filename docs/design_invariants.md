# Concord v2 design invariants

- **Audience:** maintainers and extension authors
- **Purpose:** normative cross-cutting reference

Concord v2 is a contract language that projects one semantic API model into a
domain-shaped Rust client, bounded managed execution, and inspectable
operational evidence. The [DSL specification](spec.md) owns syntax and the
[runtime model](runtime.md) owns execution detail; this page owns the laws that
cross those boundaries.

## One semantic graph

The macro resolves a raw declaration into one canonical, identity-bearing IR.
Generated Rust, Rustdoc, public descriptions, collision checks, diagnostics,
and semantic snapshots consume that IR. Emitters do not rediscover meaning
from tokens or maintain a documentation-only model.

Rust type tokens are opaque. Optionality comes only from `?`, initialization
from `#[default]` or a stable initializer, repetition from `each`, and body or
response behavior from an explicit `using` specification.

## One execution authority

Generated direct requests, bound unbound intents, runtime-defined endpoints,
endpoint-backed credential operations, retries, authentication recovery,
pagination pages, response terminals, and accepted-byte terminals use the
same core executor. No public API exposes native request or response mutation.

Request construction, descriptions, and plans are inert. Managed effects begin
only when an execution terminal is polled.

## Syntax-neutral core

`concord-macros` owns grammar, names, source spans, semantic resolution, and
projection. `concord-core` receives resolved typed operations and never parses
or recognizes DSL spelling. The hidden generated bridge is versioned and
contains semantic handoff only; v2 uses contract version 32.

## Public before private

Credential-free route, query, and header shaping and decision gates run before
private credential placement. Credential material is resolved only inside its
own bounded context and placed after rate admission. Secrets and credentials
are structurally absent from public expressions, plans, descriptions, rate
keys, gates, observers, errors, and ordinary diagnostics.

`#[redact]` marks public data, not credentials. Its real value shapes the wire
request and internal identities; every ordinary observation receives a fixed
redacted marker. A `concat` depending on a redacted value is wholly redacted.

## Immutable snapshots

Built clients, scopes, endpoint intents, and requests are immutable snapshots.
Controls consume a value and return a new one. Defaults are evaluated once at
their narrowest owning snapshot and retained; request retries, recovery,
pagination, and reset never reevaluate them.

Client clones share transport, runtime policy, credential slots, limiter,
retry budget, gates, observers, and diagnostics. Independent builds share only
handles deliberately supplied by the application.

## Capability-shaped API

A terminal or control exists only when the declaration and core can enforce
it. Pagination, accepted bytes, and credential storage are endpoint-specific.
Response acquisition is one of the sealed finite modes `Buffered`,
`Streaming`, or `Discard`.

Method names state complete guarantees. `accepted_bytes` is bounded accepted
data, not a native response; `CompleteWithin` is all-or-error if continuation
remains; dropping a future stops local polling but does not prove remote
cancellation.

## Finite behavior

Every growing dimension has a finite bound and a first-excess test: formatted
atoms, hosts, paths, queries, headers, bodies, responses, streams, records,
attempts, retry budget, pages, items, cycle state, provider traffic, canonical
keys, limiter state owned by first-party implementations, diagnostic data, and
observer queues.

Known excess fails before retention. Iterator-like unknown excess consumes at
most the first value that cannot be retained. Extension implementations must
document and enforce bounds for state core cannot own.

## Stable, distinct identities

Endpoint, rate-policy, operation, attempt, aggregate-pagination, and page
identity are distinct. Explicit endpoint and policy IDs are durable. Derived
IDs are deterministic but rename-sensitive and described as such.

Rate keys use canonical typed framing, never `Display` or `Debug`. Pagination
cycles use the real credential-free request identity; observed cycle evidence
is redacted.

## Retry and dispatch truth

Policy retry requires classifier approval plus core-owned stage,
repeatability, replayability, attempt, shared-budget, and deadline gates.
Authentication recovery is separate authority but remains within the same
attempt and deadline bounds and reacquires rate admission.

Errors report one of `NotDispatched`, `PossiblyDispatched`, or
`ResponseReceived`. Concord never presents transport knowledge as proof of
remote commit or rollback.

## One aggregate deadline

One caller-visible terminal allocates at most one absolute deadline when first
polled. It covers gates, body work, credentials, limiter waits, dispatch,
backoff, retries, recovery, acquisition, conversion, controller work, and all
pagination pages. A page cannot reset that deadline.

Completion observation happens after the result is fixed and cannot transform
it. Synchronous extension callbacks are trusted bounded work; core checks the
deadline before and after result-determining callbacks.

## Narrow extension authority

Each extension varies one named semantic decision. No extension may bypass
route ownership, authentication isolation, retry gates, deadlines, rate
admission, body or response bounds, or execution ordering. Gates may reject at
their named phase. Completion observers only record a fixed bounded event.

Runtime-defined endpoints are the dynamic-contract escape route, not a native
HTTP escape hatch. They use the same executor, planner, patch vocabulary,
limits, and response specifications as generated endpoints.

## Responsibility boundaries

- `concord-macros`: syntax, canonical IR, validation, projection, and static
  reflection.
- `concord-core`: bounded shaping, runtime snapshots, planning, execution,
  authentication, rate admission, retries, deadlines, pagination, errors, and
  observation.
- `concord-extras`: optional codecs, providers, record helpers, limiters, and
  bounded export adapters.
- `concord`: curated re-exports, feature composition, macro entry point, and
  hidden bridge path.

Changes may strengthen implementation or diagnostics, but must preserve these
laws or explicitly revise the v2 contract and its tests. There is no frozen v1
compatibility mode.
