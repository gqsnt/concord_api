# Rate limits

A contract declares the provider's quota semantics; the application selects
enforcement. Numbers and state use different declarations: reusable **window
sets** hold capacities and durations, while **policies** carry an explicit ID
and an ordered key schema that together determine which requests share state
(§12).

## Declaring policies

<!-- concord-dsl -->
```rust
client WindowSetApi {
    base "https://window-set.example.test/";
    values {
        header("X-Tenant") tenant: String;
    }
    rate_limits {
        windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
        policy tenant_reads(tenant) {
            id "catalog.tenant.read";
            key [origin, "tenant" = tenant];
            allow 120 per 1m;
            allow 5000 per 1h;
        }
        policy shared_reads() {
            id "catalog.shared.read";
            key [origin];
            using standard;
        }
    }
    rate_limit "concord.riot.21" key [origin] using standard;

    scope accounts() {
        rate_limit tenant_reads(tenant = client.tenant);
        rate_limit shared_reads();

        GET list() -> () via Empty;
    }
}
```

- A **window set** (`windows standard { ... }`) names only positive capacities
  and durations. It grants no identity and shares no state; two policies using
  the same set are separate buckets.
- A **policy** (`policy name(formals) { id ...; key [...]; cost n; windows }`)
  has an explicit ID (at most 256 bytes), a key of 1-32 uniquely labelled
  dimensions, an optional cost (default 1), and either `using set;` or inline
  `allow N per D;` windows, not both. Windows are conjunctive. Repeated window
  durations, zero or overflowing values, and a cost greater than any capacity
  are rejected. A formal that no dimension uses is an error.
- An **attachment** binds a policy where its coverage belongs:
  `rate_limit name(formal = reference);` (named bindings exact and total, with an
  optional `{ cost n; }` block), an inline `rate_limit { id ...; key [...]; ... }`,
  or `rate_limit "id" key [...] (cost n)? using set;`.
- Unused window sets and policies are shown as unused by contract inspection;
  they do not invalidate the client.

## Coverage is lexical and additive

<!-- concord-source: crates/concord-examples/src/rate_limiting/lexical.rs#rate_limit_contract -->
<!-- concord-dsl -->
```rust
api! {
    client RateLimitApi {
        base "http://rate-limit.example.test/";
        timeout 30s;

        values {
            header("X-Tenant") tenant: String;
        }

        rate_limits {
            policy account_reads(account) {
                id "example.account.read";
                key ["account" = account];
                cost 1;
                allow 100 per 1m;
                allow 1_000 per 1h;
            }
        }

        rate_limit {
            id "example.application";
            key [origin];
            cost 1;
            allow 500 per 10s;
        }

        scope region(region: String) {
            subdomain replace [region, "api"];
            rate_limit account_reads(account = region);
            rate_limit {
                id "concord.lexical.3";
                key [origin];
                cost 1;
                allow 200 per 1m;
            }

            GET item(item: String) -> Bytes via Binary {
                rate_limit {
                    id "concord.lexical.4";
                    key ["endpoint.item" = item];
                    cost 2;
                    allow 100 per 1m;
                }
            }
        }

        GET lookup(account: String) -> Bytes via Binary {
            rate_limit account_reads(account = account) { cost 2; }
        }

        GET public() -> Bytes via Binary;
    }
}
```

A policy attached at an owner covers every endpoint below it (RATE-01). No
descendant, runtime endpoint or request control can disable an ancestor
constraint, and an ID is not an endpoint selector. Here `region("eu").item(..)`
is admitted against `example.application`, `example.account.read`,
`concord.lexical.3` and `concord.lexical.4`; `lookup` against
`example.application` and `example.account.read` (cost 2); `public` against
`example.application` only.

An effective repeated attachment of one policy is deduplicated only when the
policy identity, typed binding expressions, window schema and cost are
structurally identical; a conflicting repetition on one endpoint is a compile
(or runtime-bind) error. Repetition at unrelated endpoints can intentionally
share state with different per-call costs: cost is charged, not a bucket
dimension.

## Key schemas

| Dimension | Payload |
|---|---|
| `origin` | the complete normalized logical destination `scheme://host:port` (explicit port, lowercase host) |
| `endpoint` | the operational endpoint ID; requires an explicit `id` on the endpoint |
| `"label" = value` | a required, non-page-live ordinary value, canonically encoded |
| `"label" = value via Encoder` | the value encoded by a `RateKeyEncoder` |

Custom labels (at most 128 bytes) are written explicitly and are
state-affecting: `"client.tenant" = client.tenant` and `"tenant" = client.tenant`
are different schemas even with equal values. `origin` and `endpoint` are
reserved. Optional slots, credentials, secrets, `each` sources, `concat` and
expressions cannot be dimensions. Sensitive values are eligible but always
protected in observations.

A local bucket is identified by (RATE-03):

```text
(policy_id,
 ordered [(logical_dimension_label, canonical_typed_value_bytes)],
 window_capacity,
 window_duration)
```

Cost is charged against every applicable window and is not part of the bucket.
No implicit client-instance, security-context or credential-version dimension
exists. Canonical bytes keep the baseline tagged, length-framed encoding
(`u8(tag.len) || tag || u64_be(bytes.len) || bytes`); strings use the `utf8`
tag, integers keep their exact width and big-endian bytes. A `RateKeyPart` or
`RateKeyEncoder` names its encoding with a stable `ENCODING_ID` (at most 128
ASCII bytes) that is schema metadata, not inserted into the value bytes.

## Selecting enforcement

A client with any reachable rate policy (including one available only through
runtime binding) requires an explicit limiter. There is no implicit fallback:

<!-- concord-source: crates/concord-examples/src/rate_limiting/lexical.rs#configured_rate_limiter -->
```rust
pub fn construct() -> Result<(), concord::prelude::ClientBuildError> {
    let limiter = LocalRateLimiter::new();
    let client = RateLimitApi::builder()
        .set_tenant("example-tenant")
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;
    let _: rate_limit_api::region::ItemRequest = client.region("eu").item("record-42");
    let _: rate_limit_api::LookupRequest = client.lookup("account-7");
    let _: rate_limit_api::PublicRequest = client.public();
    Ok(())
}
```

| Limiter | Enforcement |
|---|---|
| `rate_limit::LocalRateLimiter` (feature `rate-limit`) | `Local`: object-local token buckets |
| `rate_limit::NoopRateLimiter` | `Unenforced`: an explicit trusted choice, disclosed in every plan as "declared policies; enforcement explicitly disabled by application selection" |
| a custom `RateLimiter` | declares `Local`, `Unenforced` or `SharedPersistent` in its `LimiterCapability` |

Clients share local state only when given the same limiter object; separate
limiter objects count independently even with equal IDs. Clones of one client
share its limiter:

<!-- concord-source: crates/concord-examples/src/rate_limiting/partitioning.rs#shared_limiter -->
```rust
pub fn construct() -> Result<(), Box<dyn Error>> {
    let partitioned = PartitioningApi::builder()
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()?;
    let account = partitioned.regions("eu").accounts("account-42");
    let _: partitioning_api::regions::accounts::ChannelRequest = account.channel("alerts");
    let _: partitioning_api::regions::accounts::CompositeRequest = account.composite("resource-7");

    let limiter: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new());
    let first = SharedStateApi::builder()
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()?;
    let second = SharedStateApi::builder()
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()?;
    let _: shared_state_api::PingRequest = first.ping();
    let _: shared_state_api::PingRequest = second.ping();

    let bounded: Arc<dyn RateLimiter> = Arc::new(LocalRateLimiter::new().quota_capacity(1)?);
    let high_cardinality = ResourceStateApi::builder()
        .configure_runtime(move |runtime| runtime.rate_limiter(bounded))
        .build()?;
    let _: resource_state_api::ChannelRequest = high_cardinality.channel("first-key");
    Ok(())
}
```

## Admission

Each acquisition admits the complete conjunctive plan atomically or fails
without new debits (RATE-04): waiting or failed admission never reserves a
prefix of the plan. Committed admission is non-refundable, so cancellation or a
later freshness restart can consume quota without a send; a permit is an
admission result, not a held concurrency lease. Admission counts are recorded
separately from contact and send counts. The resolved key is bounded by the
minimum of the runtime setting (1,024 bytes by default), the limiter's
`preparation_key_limit` and the hard 65,536-byte bound, checked before
credentials, body or limiter effects.

Standard server feedback (the one core-parsed server delay) affects only future
admission; limiter feedback can never replace the current endpoint result.
`RateLimitResponse` carries the status and that delay, not arbitrary headers.

`RateLimitRequest::cooldown_origin` supplies an opaque, equality-only normalized
process-local origin identity independently of quota dimensions, including empty plans.
`logical_origin` remains absent unless the plan declares the `origin` dimension.
The opaque identity retains no host text and reveals none through its API,
formatting, public hashing or ordering; it is not a persistent schema key. Custom
limiter context literals now include `cooldown_origin: None`; core-generated
contexts supply the identity. This beta extension changes no bucket schema.

## The local limiter

`LocalRateLimiter` keeps bounded object-local token buckets: each capacity
replenishes continuously at capacity/duration with checked integer accounting
and no overshoot, and all windows and policies are conjunctive. A blocked plan
releases the mutex while waiting, re-evaluates on wake and stays cancellable
under the operation deadline. There is no strict FIFO fairness promise.

| Setting | Default |
|---|---|
| `quota_capacity(n)` | 4,096 quota entries |
| `cooldown_capacity(n)` | 4,096 cooldown entries |
| `idle_ttl(d)` | 15 minutes (a quota entry is prunable only when fully replenished and idle that long) |
| `key_limit(bytes)` | 1,024 resolved key bytes |

Capacity pressure rejects new state (`RateLimitErrorKind::StateCapacity`) rather
than evicting active debt or an unexpired cooldown; the required capacity of a
complete admission is checked before any insertion. Overflow, poisoning or
inconsistency fails closed.

A positive `429` delay installs an origin cooldown for an empty plan and bucket
cooldowns otherwise. If keyed cooldown feedback exceeds capacity, one finite
limiter-object-wide barrier retains the longest affected expiration. Admission
waits without any quota debit until it expires, and cancellation releases the
wait. Active cooldowns are preserved; state does not grow without bound and
recoverable capacity pressure does not permanently poison the limiter. Feedback
never changes the already-fixed response outcome.

## Shared limiters and continuity

A `SharedPersistent` adapter must atomically register and compare each policy
schema (ID, ordered labels and encoding IDs, framing version, windows,
algorithm/namespace identity) inside `acquire`, before any state use, and fail
closed on a mismatch rather than treating it as a fresh bucket (RATE-05).
Registration has finite capacity (4,096 active schemas by default for the local
conformance fixture) and never evicts active contracts. An unchanged encoding
ID is a trusted promise; Concord cannot prove opaque encoder code unchanged.

State sharing requires the same V3 adapter namespace, key framing, labels,
order, encodings, windows and algorithm interpretation. A changed ID, window
or label changes state identity. With the local limiter, a process restart
resets state by definition. Only current V3 schemas are supported; see the
[version policy](version_policy.md) and [Extensions](extensions.md) for a
custom limiter.

## Inspecting coverage

`CatalogApi::contract()` and `concord-xtask contract diff` show the exact
endpoints each policy ID covers, which endpoints can share each bucket, and
cost/window edits as separate facts (RATE-02). A hoisted attachment that newly
covers a sibling is a behavior change even when the source got shorter. A bound
`RequestPlan` shows effective policy IDs and key-label schemas without values,
and the actual enforcement choice.
