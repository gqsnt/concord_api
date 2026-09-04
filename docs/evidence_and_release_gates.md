# Evidence and release gates

The v2.1 candidate is selected for implementation, not yet for public compatibility freeze. Release requires evidence in mechanical, behavioral, security, cognitive, and performance dimensions.

## Gate 1 — Representation trait and driver prototype

Implement and compare at least:

- generic representation with sealed mode;
- generic representation with sealed driver;
- representation constructing an erased core driver.

Required pair matrix:

- JSON complete input/output;
- text and binary complete;
- `ByteStream`;
- NDJSON/CSV collected `Vec<T>`;
- NDJSON/CSV `ItemStream<T>`;
- `Empty` and `Discard`;
- optional body with nested `Option`;
- custom local representation marker;
- endpoint-backed credential output;
- accepted bytes;
- pageable complete output.

Pass conditions:

- declared MSRV compiles;
- no overlapping/coherence trap;
- no decoder/driver type leaks into ordinary caller output;
- trait errors point to endpoint, effective representation, origin, exact type, and missing role;
- downstream custom representation is possible without native response authority;
- generated token/diagnostic size stays within declared budget.

## Gate 2 — Representation inheritance

Compile/test:

- client default;
- independent request/response defaults;
- scope override;
- inline body/result override;
- `explicit` fence;
- missing representation;
- valid-but-wrong `()`/Empty/Discard cases;
- endpoint move between scopes;
- local/effective description and semantic diff.

Pass condition: effective origin is immediately visible in generated docs/errors and a scope move reports every affected leaf.

## Gate 3 — DSL and generated-DX study

Compare:

- symbolic patch operators versus `when_present`/`append`;
- `each` versus punctuation spread;
- optional controls with and without `with_x`;
- leaf-local representation versus inherited default plus inline exception.

Tasks for unfamiliar but relevant Rust users:

1. predict exact generated type/control matrix including nested `Option`;
2. add repeated query values without changing domain type;
3. preserve an inherited optional query value;
4. explicitly clear inherited repeated values with a present empty sequence;
5. locate effective representation and change one endpoint to binary;
6. move an endpoint and identify semantic changes;
7. encode one value differently in path/query/header;
8. diagnose an inherited representation trait failure;
9. distinguish no body from JSON null;
10. explain retry and pagination effect gates.

Measure correctness, time, backtracking, docs/code opened, confidence calibration, autocomplete burden, and error recovery. Do not use preference alone.

## Gate 4 — Query/header/atom properties

- alias/qualification does not change presence/cardinality;
- `RequestAtom` deterministic one-atom behavior;
- one pass invokes each atom encoder once and derives actual/sanitized forms together;
- assignment/conditional/append/remove/replace/off properties against a reference model;
- optional absence versus present empty sequence;
- case-insensitive header identity;
- reserved/public/private/media collision rejection;
- ordered sequence only; unordered types rejected;
- count and byte first-excess behavior;
- sensitive derived atom noninterference.

## Gate 5 — Normative runtime trace

Deterministic spies must prove exact order for:

### Direct success

```text
operation/deadline
public shaping
body metadata
body prepare
credential ready
attempt body
rate admission
private auth/native request
dispatch
response head
server delay/rate future feedback
auth challenge
status
representation validation/acquisition/conversion
result fixed
observer final event
result delivered
```

### Local body failure

No provider, limiter, auth placement, native request, or dispatch; `NotDispatched`.

### Retry

Pre-dispatch retry, ambiguous non-idempotent denial, safe/idempotent replay, one-shot denial, server minimum/backoff, budget timing, cancellation.

### Auth recovery

Feedback first, exact challenge/version, one source-specific recovery, new rate admission, no policy backoff/counter.

### Transport-private recovery

One visible attempt; no representation/provider/limiter/observer attempt re-entry; finite physical tries.

### Observer

Read-only/non-fallible result, bounded event cardinality, stream-open/termination separation.

## Gate 6 — Pagination proof suite

Implement:

- offset/page number with vector and envelope page;
- body cursor envelope;
- relative `Link: rel="next"`;
- custom bounded response-header cursor;
- short-page termination.

Prove:

- effect must be safe;
- complete page only;
- controller owns items/continuation atomically;
- only declared endpoint slots and response headers are available;
- cross-origin Link rejected;
- sensitive cursor internal identity/redacted evidence;
- cycle/non-progress detection;
- Prefix(0) no work/events;
- Prefix versus CompleteWithin;
- one aggregate deadline and send ceiling;
- per-page body prepare once and within-page replay;
- no inferred aggregate body reuse;
- all-or-error local result;
- malicious controller returned items/state are bounded after callback.

## Gate 7 — Rate and server-delay behavior

- canonical framed key and stable policy IDs;
- conjunctive windows;
- explicit limiter selection;
- additive inheritance and duplicate policy rejection;
- retry/auth/page attempts reacquire;
- physical recovery does not;
- Retry-After parsed once using injected time;
- retry uses max(backoff, server minimum);
- feedback failure/degradation does not rewrite current result;
- future acquire follows explicit fail-open/closed policy;
- bounded vendor-header interests;
- local state eviction/fairness/cooldown;
- no raw keys or private values in evidence.

## Gate 8 — Stream lifecycle and performance

Benchmark:

- monomorphized, boxed, and enum-erased internal drivers;
- `ByteStream` and `ItemStream<T>` wrapper overhead;
- small/large chunks and records;
- allocation count, throughput, latency;
- compile/code size;
- backpressure/cancellation;
- post-return limits and events.

Choose the private strategy based on measured target workloads. Do not alter public carrier semantics.

## Gate 9 — Information-flow security

Use unique sentinels for secrets, refreshed tokens, private auth query/header, sensitive public path/query/header, body, output, rate key, cursor, and server body.

Prove authorized-only appearance across:

- clients/scopes/intents/requests/plans/errors/streams `Debug`/`Display`;
- descriptions and semantic diff;
- observer/tracing;
- retry/rate contexts;
- pagination identity errors;
- body/codec/provider/transport errors;
- runtime endpoint builder/bind;
- endpoint-backed storage result.

Sensitive public values must still affect the real request/internal identity.

## Gate 10 — Large contract, docs, and release hygiene

Use Riot and generated 1/10/100/500/1,000 endpoint fixtures. Measure:

- authored representation lines;
- generated method count with/without `with_x`;
- token/expanded source size;
- clean/incremental compile time and memory;
- invalid-pair diagnostic size/time;
- rust-analyzer responsiveness;
- descriptor/binary size;
- request shaping and retry/pagination overhead;
- generated Rustdoc first-screen clarity;
- parent-change semantic diff.

Release hygiene:

- one canonical language across code/docs/examples;
- provisional aliases removed;
- version bumped to 2.1.0;
- MSRV/feature matrix passes;
- no native escape or unbounded path;
- migration guide complete;
- all selected ADRs accepted or revised with evidence.

## Repository verification

Run where available:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just docs
just deny
just workflow
just analyzer
just bench
just verify
```

Report exact commands and results. Do not claim unavailable checks passed.

## Release decision

Freeze only when all ten gates pass or a documented ADR narrows/replaces a gate with evidence. A failed gate changes the design; it is not solved by adding compatibility aliases or more prose.

