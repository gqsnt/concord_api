# Concord v2.1 mental model

## One contract, several views

A Concord declaration is compiled once into task-specific views:

```text
source tree
  = generated navigation tree
  = unbound navigation tree
  = contract-description tree
  = documentation tree
```

The views differ in authority, not meaning.

## Static contract state

- client/scope/endpoint tree;
- exact Rust value declarations;
- presence, initializer, sensitivity, and owners;
- representation defaults, scope fences, and local overrides;
- route/query/header/auth/rate patches;
- status and effect;
- body/response/pagination capabilities;
- stable IDs and provenance.

## Built-client state

- client-owned values and retained defaults;
- fixed logical origin;
- transport/runtime configuration;
- shared retry budget and selected limiter;
- versioned credential slots;
- read-only observer and diagnostic policy;
- security-context identity.

Clones share these handles according to documented semantics. Independent builds are separate security contexts unless the application explicitly supplies shared services.

## Inert selected state

A scope, endpoint intent, or request snapshot contains immutable selected values and local controls. Construction, cloning, description, and planning do not perform HTTP, provider, limiter, body preparation, retry, or observation work.

## Logical operation

Polling a terminal creates one operation identity and absolute deadline. A direct operation may contain visible attempts. A pagination terminal contains page operations, each of which may contain visible attempts.

```text
operation
  ├─ visible attempt
  │    └─ optional transport-private physical recovery
  ├─ policy retry
  ├─ auth-recovery replay
  └─ fixed outcome
```

## Effect class

- `safe`: requested semantics are read-like; may paginate and may retry when other gates allow;
- `idempotent`: requested mutation can be repeated with the same intended effect; may retry but ordinary pagination is unavailable;
- `non_idempotent`: ambiguous-dispatch policy retry is prohibited.

Effect is endpoint meaning. Body replayability is a separate mechanical fact.

## Public and private information

```text
ordinary public protocol data
sensitive public protocol data
body/content data
private credential data
operator-only bounded diagnostics
```

Sensitive public data still reaches the server and may affect internal correctness identities. It is redacted from automatic plans, errors, observation, and debug output. Credential material never enters public shaping or general observation.

## Representations and delivery

Request and response representations describe wire/media conversion. Exact endpoint types describe caller values and lifecycles.

```concord-pseudocode
response Json;
GET item() -> Item;                 // complete value

response NdJson;
GET snapshot() -> Vec<Event>;       // complete collection
GET watch() -> ItemStream<Event>;   // lazy semantic stream

response Binary;
GET download() -> ByteStream;       // lazy bytes
```

## Pagination

Pagination is an explicit safe aggregate. The controller receives one complete accepted page plus a bounded declared metadata view, then returns aggregate items and either completion or the next controller state. It never receives a native response or arbitrary request mutation.

## Plans and descriptions

- contract description: static value-free declaration;
- effective endpoint description: inherited normalized contract and provenance;
- request plan: bound credential-free public request projection;
- operation evidence: runtime identities, stages, waits, and outcomes.

Do not use one concept for all four.
