# Extension contracts

Concord extensions implement semantic work at a finite typed boundary. They do
not receive native transport authority, generated internals, or permission to
replace the managed executor. First-party implementations in
`concord-extras` obey the same contracts as application code.

## Purpose

This contract lets codecs, bodies, records, credentials, pagination, limiters,
and hooks evolve independently of the native HTTP implementation. It also
defines the trust that remains after core has bounded its own inputs:

- which callbacks are synchronous or asynchronous;
- which work is under the operation deadline;
- what cancellation can stop;
- which application state and side effects remain outside core's control;
- how failures cross the public boundary.

The task-oriented trait selector and implementation skeletons are in
[Extending Concord](../../extending/README.md). This page is the maintainer
checklist for preserving all trait families together.

## Public model

Choose the smallest family that owns the behavior:

| Semantic work | Contract family | Core retains |
|---|---|---|
| Complete buffered request encoding | `EncodeBody` | body limit, request lifecycle, transport |
| Complete buffered response decoding | `DecodeBody` | status and finite acquisition |
| Request metadata and managed preparation | `RequestBodySpec`, `BodyFactory` | final body convergence and native conversion |
| Successful response conversion | `ResponseBodySpec` | sealed mode, status, body acquisition |
| Incremental record framing | `RecordDecoder` | bounded byte stream and staging progress |
| Credential material and acquisition | `CredentialMaterial`, `CredentialProvider` | slots, placement, attempts, provider HTTP |
| Page continuation | `PaginationController` | page loop, identities, collection, policy |
| Admission and response policy | `RateLimiter`, `RateLimitObserver` | plan validation and lifecycle ordering |
| Lifecycle observation and veto | `RuntimeHooks` | effects, result, completion dispatch |

Application crates reach these contracts through `concord::*` façade paths.
They should not depend on `concord-core` to find public traits or on
`concord-extras` merely to implement one.

## Internal flow

Extension calls are adapters around core-owned stages:

```text
typed public input
        |
        v
finite semantic context -> extension callback -> approved semantic result
        |                                        |
        +---------- safe error reduction <-------+
                                                 |
                                                 v
                                      core-owned next lifecycle stage
```

The context contains only what the operation needs: an encode/decode limit,
structural request metadata, a successful response head, bounded staged bytes,
credential-provider HTTP, immutable quota plans, or sanitized lifecycle
events. Native request mutation, redirects, transport handles, authenticated
URLs, native error chains, and completion dispatch stay inside core.

Generated endpoints and runtime-defined endpoints converge before these calls.
An extension does not need a generated adapter and must not infer which
construction path produced the request.

## Architectural invariants

- Every extension family is semantic and transport-neutral.
- Context types have deliberate finite fields and exclude unrelated private
  state.
- Core validates extension results again where a result can affect a native
  boundary, retained bytes, replayability, or response mode.
- Synchronous callbacks are never treated as preemptible merely because a
  deadline exists.
- Asynchronous result-determining callbacks run under the remaining operation
  deadline unless their contract explicitly says otherwise.
- Completion and pagination-controller callbacks are trusted outside the
  per-request deadline.
- Dropping a future or stream stops later Concord work but cannot roll back
  completed extension effects.
- Calls from independent operations may overlap. `Send`/`Sync` bounds are not
  a promise of serialized invocation.
- Extension errors are reduced to stable finite public structure before they
  leave the owning stage.
- First-party extras have no friend access to native transport or core-private
  state.

## Bounds

Finite input does not imply finite extension behavior. Core can cap supplied
bytes, key components, headers, provider bodies, and staged records, but it
cannot automatically cap:

- output heap size or internal representation expansion;
- caches, maps, queues, retry counters, or batch retention;
- metric label cardinality;
- synchronous CPU or blocking work;
- spawned tasks and their lifetime;
- external service latency when an operation timeout is disabled.

Each trait's Rustdoc states which callback and state owner carries these
obligations. The cross-cutting policy is summarized in
[Finite behavior](../../concepts/finite-behavior.md); exact core-owned values
are in [Limits](../../reference/limits.md).

An extension must stop at the first input that exceeds a declared bound when
the contract requires first-excess withholding. It must not fully collect an
untrusted iterator and reject later.

## Errors and redaction

Treat extension inputs as potentially sensitive even when credentials are
absent. Public URLs, response headers, records, decoded values, pagination
state, and quota keys can identify customers or operations.

Error values should contain a stable kind and only explicitly reviewed safe
text. Drop parser, serializer, native, provider, or remote error sources before
constructing a public extension error unless the contract has a dedicated
safe reduction. `Debug` and `Display` require separate sentinel tests.

Sanitized headers and URLs are safe from Concord-managed credential placement,
not from all application data. An extension owns its logs, metrics, traces,
storage, and retention. The canonical model is
[Security and redaction](../../concepts/security-and-redaction.md).

## Extension boundary

The boundary is intentionally not a general plugin API. An implementation may
compute its semantic result, call the bounded provider HTTP capability where
documented, or observe the supplied event. It may not:

- construct or send the application's native HTTP request;
- read or mutate protected authentication placements;
- bypass limiter admission or response observation;
- replace status handling, finite response acquisition, or completion;
- forge operation identity or generated endpoint authority;
- return a native response, transport client, or unbounded stream;
- rely on core-private or generated-private paths.

## Failure modes

Every extension family needs proofs for:

- exact boundary and first-excess inputs;
- redacted `Display` and `Debug` with unique sentinels;
- callback panic-free safe error reduction;
- overlapping calls and shared-object behavior;
- timeout and cancellation at every asynchronous boundary;
- no detached correctness work after the owning future or stream is dropped;
- synchronous callbacks that return after a deadline has elapsed;
- invalid extension output being rejected before native effects or retention;
- use through both generated and runtime-defined endpoint paths where
  applicable.

Tests should use deterministic loopback or scripted transports, paused time,
manual polling, and bounded iterators. An external service is not a contract
proof.

## Change hazards

Adding a context field grants every implementation a new observation
capability. Review sensitivity, cardinality, ownership, borrow lifetime, and
whether the field could expose native or protected state.

Adding a callback introduces a new ordering and liveness edge. Define its
deadline, cancellation, overlap, failure reduction, completion behavior, and
test owner before stabilizing the signature.

If a desired concrete implementation fits an existing trait, place it in
`concord-extras` behind the narrowest feature. If it needs privileged native
state, first ask whether the semantic trait is missing a safe abstraction.
Do not create a private extras-only bridge.

Public trait changes also affect Rustdoc, doctests, downstream examples,
feature-isolation fixtures, no-default builds, MSRV, and the generated
contract when emitted code names the trait. Follow
[Adding a feature](../adding-a-feature.md) and
[Feature composition and evolution](../architecture/feature-composition.md).

## Verification

Proof ownership is distributed deliberately:

- `crates/concord-core/tests/public_api/` proves stable trait reachability,
  sealed/private surfaces, and finite context;
- core module and executor tests prove result validation, ordering,
  cancellation, and error reduction;
- `crates/concord-extras/src/` unit tests prove concrete downstream behavior
  without private access;
- `crates/concord/tests/extensions/` proves façade paths and cross-feature
  composition;
- `crates/concord-examples/src/` owns canonical compiled implementation
  skeletons;
- façade and core UI suites prove that native and private capabilities do not
  escape;
- fixtures prove feature isolation, renamed-façade behavior, and generated
  contract mismatch where relevant.

Run workspace doctests and warning-denied Rustdoc in addition to unit tests:
trait documentation is part of the extension contract. Use the complete
matrix in [Testing strategy](../testing.md).

## Related

- [Authentication contract](authentication.md)
- [Bodies, responses, and finite progress](body-response-progress.md)
- [Records and pagination contract](records-pagination.md)
- [Rate-limit contract](rate-limits.md)
- [Errors, redaction, hooks, and completion](errors-hooks-completion.md)
