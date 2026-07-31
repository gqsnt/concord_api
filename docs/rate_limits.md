# Rate limits

- **Audience:** API authors, callers configuring a limiter, and limiter authors
- **Purpose:** end-to-end concept guide
- **Normative reference:** [DSL rate-limit rules](spec.md#rate-limit-profiles-and-policy)

Rate limiting separates reusable policy declaration, local attachment and
binding, resolved semantic policy, and runtime enforcement.

## Contract example

<!-- concord-source: crates/concord-examples/src/rate_limiting/lexical.rs#rate_limit_contract -->
```rust
api! {
    client RateLimitApi {
        base "http://rate-limit.example.test/";
        vars {
            tenant: String;
        }
        rate_limit {
            id "example.application";
            500 / 10s;
        }
        rate_limits {
            rule account_reads {
                id "example.account.read";
                by [account];
                cost 1;
                100 / 1m;
                1_000 / 1h;
            }
        }
    }

    scope region(region: String) {
        host [scope.region, "api"];
        rate_limit use account_reads {
            account = scope.region;
        }
        rate_limit {
            by [host];
            200 / 1m;
        }

        GET item(item: String) -> Bytes {
            rate_limit {
                by [ep.item];
                cost 2;
                100 / 1m;
            }
        }
    }

    GET lookup(account: String) -> Bytes {
        rate_limit use account_reads {
            account = ep.account;
            cost 2;
        }
    }

    GET public() -> Bytes;
}
```

A named profile declares stable policy shape and formal logical dimensions. A
`rate_limit use` attachment binds every formal to values visible at its tree
site. A small local policy may be anonymous.

Profiles do not capture `vars.*`, `scope.*`, or `ep.*` directly. Explicit
formals make dependencies, reuse, compatibility, documentation, and generated
trait obligations visible.

## Why policies are additive

Effective policy order is client, outer scope, inner scope, then endpoint.
Descendants cannot disable inherited policies. A remote quota commonly applies
at several simultaneous boundaries—application, host, service, endpoint, or
resource—and allowing a leaf to opt out would make the least local declaration
able to bypass shared authority.

## Dimensions and keys

Dimensions may use `host`, `endpoint`, or required/defaulted scalar values.
Their names and order are part of policy compatibility.

Optional values, vectors, formatted expressions, secrets, credentials, and
pagination-controlled values are rejected. These shapes would make policy
schema, cardinality, privacy, or page-to-page identity unstable.

Resolved key values may contain tenant, account, host, region, or endpoint
data. Built-in formatting and errors suppress them. Custom limiters and
observers are trusted to protect storage and logs.

## Cost and windows

Cost is positive and must not exceed any window capacity. A policy has at least
one positive ordered window. Explicit IDs allow intentional sharing across
source renames or clients when the application guarantees schema and remote
quota compatibility.

## Configure enforcement

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

The façade `rate-limit` feature installs a bounded `LocalRateLimiter` in each
built client unless an explicit limiter is configured. Without a selected
limiter, an empty plan executes and a nonempty plan fails closed.

Default-built clients do not share state. Supplying the same limiter object to
several clients shares compatible policy, key, and window state.

## Runtime order

Each visible attempt acquires every effective policy before private
authentication placement. Retries and authentication recovery reacquire.
Pagination recomputes and acquires policy for every page.

`RateLimitObserver` receives the sanitized response head and resolved policy
plan before the user response hook. It may update cooldown or quota state and
may reject the operation with a safe error.

## Local versus distributed limiting

`LocalRateLimiter` implements bounded process-local token buckets with finite
state, pruning, and deterministic eviction. It is not a distributed strict
quota. A distributed implementation uses the same `RateLimiter` contract and
owns cross-process consistency and its own finite state.

## Guardrails

The structured policy model enables:

- complete binding of reusable formals;
- validation that every named profile is used;
- stable ID/schema compatibility checks;
- bounded dimensions, keys, costs, and windows;
- secret and pagination-dependency exclusion;
- deterministic additive ordering;
- fail-closed enforcement when policy exists but authority does not.

## Related

- [Retry](retry.md)
- [Pagination](pagination.md)
- [Execution policy](execution_policy.md)
- [Extensions](extensions.md)
