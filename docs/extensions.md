# Extensions and `concord-extras`

An extension is a public language. Each extension varies one semantic decision while core retains native execution, ordering, private authority, and finite resources.

## General extension contract

Every public extension documents:

- semantic role and who consumes it;
- exact inputs and information classification;
- decisions/mutations it may perform;
- state it may retain;
- synchronous/async/cancellation obligations;
- resource bounds;
- stable error contract;
- concurrency/`Send`/`Sync` semantics;
- properties core still guarantees;
- negative capabilities;
- compatibility/sealing policy.

No universal context, native request/response, arbitrary header map, or runtime mutation map.

## Synchronous trust boundary

Synchronous extension methods must terminate promptly, bound CPU/allocation, avoid blocking/external I/O, and avoid detached work. Core checks deadlines before/after but cannot preempt arbitrary Rust code.

Async extension futures are polled under the remaining deadline/role cap and must be cancellation-safe.

## `RequestAtom`

Owns one canonical public scalar representation. It does not decide URL context, omission, repetition, rate identity, body encoding, auth, or retry.

Extras may provide:

- `DisplayAtom` explicit compatibility adapter;
- common time/ID/enum adapters;
- bounded joined-collection adapters.

Do not provide a blanket `Display` implementation.

## `OrderedValues`

Owns stable by-reference sequence order for `each`. It grants no query/header behavior until source explicitly writes `each`.

Implement for clearly ordered standard sequences, not unordered maps/sets or every `IntoIterator`.

## Request representation

`RequestRepresentation<Input>` owns input-to-managed-body conversion, media metadata, and declared replay/page capabilities. It does not see public route/auth/rate/native request.

Core owns body invocation count, limits, cancellation, visible attempt reconstruction, and native adaptation.

## Response representation

`ResponseRepresentation<Output>` owns accepted-head media validation and conversion through a supported sealed core driver. It does not own status acceptance or native response polling.

Core owns acquisition modes, bytes/records/progress, cancellation, streams, and status order.

## Sealed driver families

Core may support a finite internal algebra:

```text
CompleteDocument
CompleteBytes
CollectRecords
StreamRecords
StreamBytes
ValidateEmpty
DiscardBody
```

External representations may configure/select supported semantic adapters but cannot implement a driver that bypasses core.

## Credential provider

Receives one acquisition reason, optional current material for refresh, isolated provider HTTP, and remaining deadline. It owns one credential-source protocol only.

No application client, endpoint body/output, limiter, observer, native request, or final application URL.

## Retry classifier

Receives bounded semantic failure facts and recommends whether a policy retry is useful. Rejected-status input contains status and the standard parsed server delay, never arbitrary response headers. It cannot grant authority. Core still applies effect, dispatch, body, count, load-budget, and deadline gates.

## Rate limiter

Receives complete effective policy plan, protected canonical keys, cost/windows, IDs, and deadline/cancellation. Admission may wait/reject/fail. Feedback is limited to typed status/server-delay/plan/attempt facts, is non-fallible for the current result, and only influences future policy state.

No body, credentials, native request, response-header map, or unrestricted response.

## Pagination controller

Receives one complete `PageResponse<Page>` containing exact decoded page and only declared bounded metadata interests. It returns items plus completion/next state atomically.

No client, auth, rate, native response, arbitrary headers, or request mutation.

## Operation observer

Receives bounded borrowed sanitized events and returns unit. It cannot mutate, veto, retry, or perform async work through core. Async export is an extras/application-owned bounded handoff.

## First-party extras

### Representations

- `Json`
- `Text`
- `Binary`
- `Form`
- `Empty`
- `Discard`
- `NdJson`
- `Csv<Cfg>`
- managed multipart and request-stream roles where features are enabled

Use zero-sized generic markers where Rust coherence/diagnostics pass the prototype gate. Do not expose decoder/driver wrapper towers in caller signatures.

### Streams

- `ByteStream`
- `ItemStream<T>`
- `RequestStream<T>`

Keep decoder/driver internals private. Benchmark boxed versus enum/static erasure before freezing.

### Safe JSON diagnostics

Use structured phase/reason and bounded sanitized paths. No raw payload or arbitrary `serde_json` error string in automatic public output.

### Pagination

Provide offset, page-number, cursor-body, short-page, Link-header, and declared-header controllers. Link controllers reject cross-origin continuation.

### OAuth2

One isolated credential provider with fixed/validated token origin, injected time, finite body limits, and no application client identity.

### Local rate limiter

Process-local implementation with bounded state, deterministic clock, documented fairness/eviction, and explicit name. Never imply distributed enforcement.

### Tracing/OpenTelemetry adapter

Maps `OperationEvent`/stream events to safe bounded fields. It does not own exporter threads/queues in core and does not emit high-cardinality/raw values.

### Test support

Deterministic transport, clock, provider, limiter, observer, body, record, and pagination fixtures exercise the same public/core semantics. Test support must not create a production native mutation path.

## Cargo features

Features add integrations/capabilities. They do not silently alter existing endpoint meaning, install policies, or create incompatible product personalities.

Document no-default and focused feature combinations, MSRV, public dependency types, and default decompression behavior.

## Sealing

Seal traits whose downstream implementation would expose unstable internals or bypass limits/security/ordering. Keep semantic roles implementable when downstream implementations are part of the product. Document the choice.
