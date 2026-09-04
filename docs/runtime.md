# Runtime model

## Built client

A successful build creates one immutable runtime/security snapshot:

```text
ClientCore (shared handle)
  transport pool and HttpConfig
  ProviderHttpConfig
  RuntimeConfig and finite limits
  retry policy/shared load budget
  selected limiter
  credential source/slot state
  observer/diagnostic policy
  contract and security-context identity

client public value frame
  retained immutable values/defaults
```

Client clones share these handles. Independent builds are independent security contexts unless explicit shared service handles are supplied.

## Scope and request snapshots

Scopes and requests are immutable selected domain snapshots. Controls consume one value and return another. Cloning is generated only when semantically valid; one-shot body input is not required to implement `Clone` merely for symmetry.

## Configuration owners

### `HttpConfig`

Application transport:

- HTTPS/HTTP policy;
- proxy and shared root trust;
- application client identity;
- protocol selection and transport-private recovery;
- connect/read-idle/pool/TCP controls;
- managed decompression.

### `ProviderHttpConfig`

Credential-provider transport:

- allowed origins;
- finite request/response/phase limits;
- explicit shared/overridden proxy and root trust;
- provider-specific retry where semantically required;
- never application endpoint credentials or application client identity.

### `RuntimeConfig`

- body/response/status/stream/record limits;
- pagination page/item/byte/send/identity limits;
- auth/rate cooldown limits;
- observer and diagnostics;
- operation identity generation;
- runtime-endpoint structural limits.

## Defaults

- finite everywhere;
- retry absent until explicitly selected;
- observer no-op until selected;
- structural/sanitized debug only;
- clients with rate policy require explicit limiter;
- post-content-decoding byte accounting;
- no detached correctness task;
- redirects disabled;
- safe transport-private recovery may be enabled by documented default and explicitly disabled.

## Client build validation

Before returning a client, validate without external I/O:

1. required public values;
2. required secret/provider inputs;
3. fixed origin and static contract consistency not already guaranteed by expansion;
4. finite timeout/resource configuration;
5. explicit limiter selection;
6. HTTP/provider/runtime cross-constraints;
7. bounded static metadata.

Return one bounded actionable `ClientBuildError` aggregating missing/conflicting construction facts where practical.

## Resource accounting

Independently bound:

- URL labels/segments/query/header counts and bytes;
- request body bytes/parts/progress;
- response/status/accepted bytes;
- stream bytes/items/progress/read idle;
- attempts, physical tries, backoff, and retry load;
- credential work/cooldown/waiters;
- rate policy dimensions/keys/windows/state/waiters;
- pagination pages/items/accepted page bytes/sends/identities;
- observer fields/events and trusted diagnostics;
- runtime-endpoint dynamic structure.

## Transport ownership

Core alone owns native request/response, TLS/proxy/pool/socket behavior, content decoding, and HTTP protocol recovery. Extensions receive semantic bounded views only.

## Planning and execution

Planning and execution consume the same semantic shaping program. Each pass may encode independently under the deterministic `RequestAtom` contract. Within one pass, actual and sanitized forms derive from one atom emission.

A public frozen resolved request is deferred until signing/auditing or nondeterministic adapter evidence requires byte-identical plan/execution.
