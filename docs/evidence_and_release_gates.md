# Evidence and release gates

The v2.1 integration candidate is selected for implementation, not yet for
public compatibility freeze. The canonical current disposition is
[`dev_doc/release_gate_status.md`](../dev_doc/release_gate_status.md). Release
requires evidence in legal/package, mechanical, behavioral, security,
cognitive, platform, and performance dimensions.

## Gate 0 — Package identity, legal authority, and release namespace

Required owner inputs and proof:

- explicit license policy implemented without a self-invented waiver;
- approved and currently viable public package names, with registry control
  distinguished from mere absence;
- first-publication and v2.0 external-consumer history;
- official repository/homepage/readme metadata;
- final archive, dependency-order, local-registry, downstream, Rustdoc, and
  `cargo deny` evidence;
- `publish = false` retained unless a separate task authorizes publication.

Gate 0 is mandatory. Engineering may prepare the packet and archives, but only
the project owner can supply the legal and product decisions.

## Gate 1 — Representation trait and driver proof

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

## Gate 3 — Unfamiliar-human final-surface comprehension

Use at least three unfamiliar but relevant Rust users on the one selected final
surface. The maintained protocol and scoring packet lives under
`dev_doc/dx_study/`. It covers exact/nested optional controls, repeated atoms
and patch operations, representation provenance and fences, effect/retry,
pagination completeness, stream lifecycle, diagnostics, authority boundaries,
and semantic diff. Measure correctness, time, confidence, documentation/search
path, wrong models, recovery, and abandonment without coaching.

Pass only when no critical misconception is shared by two participants, every
critical task is correct for at least two of three, overall correctness is at
least 80%, and all release-relevant Critical/Major findings are fixed and
retested. `with_x` remains deferred; reopening it requires a separate paired,
counterbalanced study with at least four to six participants.

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
- rate feedback contains only typed status/server-delay/plan/attempt facts;
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
Also use an unknown response-header-name/value sentinel so the test cannot pass
only because of a sensitive-name denylist.

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

## Gate 10 — Large contract, docs, release hygiene, and independent review

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

Supported-platform execution on Windows, Linux, and macOS is mandatory. An
independent Rustdoc/semantic-diff reviewer must also find construction,
optional/default controls, directional representation provenance/fences,
parent-change consequences, semantic-diff classifications, pagination and
stream completeness, and metadata-authority boundaries without macro-source
reading. AI and the primary implementer do not count.

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

Freeze only when Gate 0 through Gate 10 all pass. A failed gate is not solved by
adding compatibility aliases, prose without evidence, or an unauthorized
waiver.
