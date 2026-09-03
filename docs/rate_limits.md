# Rate limiting

The declaration defines stable quota semantics; a selected runtime limiter
implements admission, waiting, rejection, and coordination.

## Profiles and attachments

Profiles are client-owned and name every formal. They cannot capture client,
scope, or endpoint values. An attachment binds each formal exactly once and may
override cost. Anonymous attachments declare one policy inline.

`id` is stable policy identity. `key` is an ordered dimension schema. `cost`
defaults to 1, must be positive, and cannot exceed any window capacity. Every
policy has at least one positive `allow capacity per duration` window.

Policies add across client and nested scopes/endpoints. A descendant cannot turn
off inherited quota semantics. Duplicate effective use of one ID is rejected;
reuse elsewhere requires compatible ordered dimensions and windows.

## Canonical keys

Built-in dimensions are final logical `host` and stable `endpoint` ID. Other
dimensions are direct total owner references or bound profile formals. Outer
optional slots and pagination-mutated values are rejected, as are secrets,
credentials, concatenation, and display formatting.

`RateLimitKeyValue` writes bounded deterministic bytes with type and length
framing. First-party primitives and structural `Option`/`Vec` encodings keep
variants and boundaries distinct. A required `Option<T>` payload may therefore
encode `None` as real domain identity; a Concord-optional slot cannot make policy
presence data-dependent.

Canonical values are supplied only to the limiter. They do not appear in plans,
errors, gates, completion, descriptions, or Rustdoc.

## Enforcement

Any generated client with an effective rate policy requires explicit limiter
selection. There is no implicit allow-all implementation and no production
bypass method. Tests can select the explicit test limiter from test support.

<!-- concord-source: crates/concord-examples/src/rate_limiting/lexical.rs#configured_rate_limiter -->
```rust
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter =
        LocalRateLimiter::new().observer(Arc::new(super::observer::DelegatingRetryAfterObserver));
    let client = RateLimitApi::builder()
        .set_tenant("example-tenant".to_owned())
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest =
        client.region("eu".to_owned()).item("record-42".to_owned());
    let _: rate_limit_api::LookupRequest = client.lookup("account-7".to_owned());
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
```

Admission occurs once per actual network attempt, including policy retry and
authentication recovery, inside the operation's aggregate deadline. Private
credential placement follows admission. Response-head limiter observation may
update future cooldown state but cannot replace the current response.

Client clones share their selected limiter and budgets. Independent builds
share quota state only if the application supplies the same limiter handle.

See the exact [rate grammar](spec.md#rate-limits) and
[runtime admission model](runtime.md#rate-admission).
