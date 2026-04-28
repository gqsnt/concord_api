# Public API Boundary

Concord v4 has three public layers.

## `concord_core::prelude`

Normal generated-client users import this:

```rust
use concord_core::prelude::*;
```

The prelude intentionally contains:

| Item group | Reason |
| --- | --- |
| `ApiClient`, `ApiClientError`, `ClientContext`, `Endpoint` | Core public client and explicit endpoint usage |
| `Json`, `Text`, `NoContent` | Common codecs |
| `AccessToken`, `ApiKey`, `BasicCredential`, `SecretString` | Common credential materials |
| `PendingRequest`, `PaginatedRequest` | Public request builder/stream types |
| `DebugLevel` | User-facing debug level |
| `RateLimitObserver`, `RateLimitObservation`, `RateLimitResponseContext` | Supported rate-limit extension point used directly by DSL users |
| Built-in pagination controller names | Needed by `paginate ...` declarations |

## `concord_core::advanced`

Extension authors import this:

```rust
use concord_core::advanced::*;
```

This layer contains transport, cache, credential provider, runtime hook, retry, rate-limit, inflight, and low-level runtime integration traits.

## `concord_core::internal`

Generated code uses this layer. Normal examples and docs should not import it.

This layer contains request-plan plumbing such as `EndpointPlan`, `RequestPlan`, `ResolvedRoute`, `ResolvedPolicy`, `BodyPlan`, `ResponsePlan`, and low-level codec helpers.

