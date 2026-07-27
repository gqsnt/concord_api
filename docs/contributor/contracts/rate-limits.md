# Rate-limit contract

Rate limiting is correctness-relevant admission policy inside the managed
executor. A declaration or runtime endpoint supplies a finite immutable plan;
a selected limiter admits each visible attempt and observes its final response
head. Core owns plan validation, lifecycle ordering, safe context, and error
reduction.

## Purpose

The contract supports reusable quota implementations without granting them
request mutation or transport access. It also makes policy composition
predictable:

- declaration inheritance and endpoint clauses decide the effective generated
  plan;
- runtime endpoint builders construct the same semantic plan directly;
- the client snapshots a limiter and its preparation limit;
- every direct request, authentication recovery, and pagination page requests
  admission through that limiter.

The application workflow is in
[Rate limiting](../../features/rate-limiting.md). This page owns the
compiler/runtime and lifecycle obligations.

## Public model

The semantic plan uses:

- `RateLimitId` for a bounded deterministic rule identity;
- `RateLimitKey` and ordered `RateLimitKeyPart` values for partitioning;
- `RateLimitWindow` for one positive capacity over one positive duration;
- `RateLimitRule` for identity, key, cost, and finite windows;
- `RateLimitPlan` for the ordered effective rules of one attempt.

`RateLimiter` supplies asynchronous `acquire` and `observe` callbacks.
`RateLimitRequest` contains endpoint, method, credential-sanitized URL, logical
host, page index, `AttemptInfo`, and the immutable plan. `RateLimitResponse`
adds status, sanitized headers, and the centrally configured cooldown cap.
`RateLimitResponseHead` exposes the same attempt identity to synchronous local
observers.

The returned `RateLimitPermit` is proof that one visible attempt was admitted.
It is not a concurrency lease retained until response completion.

`RateLimitObserver` is a synchronous policy helper used by the first-party
`LocalRateLimiter`. It may propose a bounded `Cooldown` over current buckets,
the logical host, or the limiter-wide client scope. Custom limiters need not
use that helper.

## Internal flow

Generated code resolves inherited declarations, endpoint attachments, costs,
windows, and allowed public key dimensions into an immutable plan. Runtime
endpoint construction uses public plan builders but is validated against the
same structural ceilings.

Client construction asks the selected limiter for
`preparation_key_limit`, snapshots that value with the limiter object, and
rejects a value above the hard aggregate-key ceiling. Zero is meaningful: it
allows global keys and rejects any partitioned key.

For each visible attempt, core:

1. prepares and validates the complete plan before credentials, body
   materialization, or transport;
2. runs `before_send` and request-body preparation;
3. completes authentication preflight without attaching material;
4. awaits `RateLimiter::acquire` under the remaining operation deadline;
5. materializes exact-current credentials and sends the request;
6. invokes built-in debug response observation;
7. awaits `RateLimiter::observe` on the sanitized final response head;
8. only then invokes the user response hook, challenge handling, status
   handling, and body acquisition.

Observation cannot replace or mutate the received response. Its failure or
timeout nevertheless determines the operation before application response
handling proceeds.

Before managed policy retry exists, the initial request carries
`AttemptInfo { index: 0, policy_retry_index: 0,
authentication_recovery: false }`; the one authentication replay carries
`{ index: 1, policy_retry_index: 0, authentication_recovery: true }`.
Acquisition, asynchronous response observation, hooks, and synchronous local
observation see the matching value.

## Architectural invariants

- Plans and keys are finite, validated semantic values with no native HTTP
  state.
- Resolved key values may be sensitive; key and plan types do not expose
  automatic `Debug` or `Display` that would reveal them.
- Final endpoint validation is authoritative for generated dimensions,
  including inherited rules and host expressions that depend on
  pagination-controlled values.
- Generated rule IDs are deterministic within one generated client contract;
  they are not a global namespace across unrelated macro expansions.
- Limiter preparation happens before any attempt effects, and admission
  happens before protected credential attachment.
- Every application-level authentication replay acquires a new permit.
- Response observation precedes user hooks, authentication recovery, terminal
  status conversion, and successful body work.
- Limiter failures use the ordinary completion path and cannot bypass
  redaction.
- Limiter objects are shared by client clones. State partitioning follows
  rule/key/host semantics, not generated wrapper identity.

## Bounds

Hard limits cover IDs, key parts, aggregate key bytes, windows per rule, and
rules per plan. Runtime configuration bounds server-directed cooldown and the
first-party limiter's retained quota and cooldown state. The
[limits reference](../../reference/limits.md) is the exact inventory.

The first-party local limiter must reject new state recoverably at capacity.
Insertion is transactional: a failed attempt cannot leave a partial collection
of buckets. Safe pruning may remove idle, fully replenished state, but may not
erase active debt or cooldown.

Custom limiters own their memory, queues, metrics, retry loops, external I/O,
and spawned tasks. `acquire` and `observe` futures run under the remaining
operation deadline and are dropped on cancellation; completed reservations and
external effects are not rolled back. With the timeout disabled, custom
limiter liveness is trusted.

## Errors and redaction

Invalid plans, key construction, acquisition, observation, timeout, and
first-party capacity failures reduce to `ErrorCategory::RateLimit` with a
focused `RateLimitErrorKind`. Error strings do not include resolved keys,
logical hosts, observer header values, or custom error text.

`safe_url` is credential-sanitized, and response headers are exposed through
`SanitizedHeaders`. Those values may still contain public identifiers or
personally identifiable information. A limiter or observer owns how it stores,
logs, and labels them.

The exact public categories and focused kinds belong to
[Errors](../../reference/errors.md); the confidentiality model belongs to
[Security and redaction](../../concepts/security-and-redaction.md).

## Extension boundary

Implement `RateLimiter` for a new admission algorithm or remote authority.
Implement `RateLimitObserver` when extending the synchronous response-header
interpretation of `LocalRateLimiter`. Neither trait receives credentials,
request bodies, native requests, native responses, hook objects, or control of
the executor.

Calls may overlap. An implementation must define safe sharing and atomicity for
its own state. Synchronous work before a returned future and observer callbacks
cannot be preempted, so they must be short and finite.

`NoopRateLimiter` is a deliberate policy that accepts any plan without local
enforcement. The internal fallback used when no configured implementation can
serve a nonempty plan rejects that plan; it is not equivalent to opting into
no-op behavior. The default façade feature installs the first-party local
implementation without moving semantic ownership out of core.

See [Rate limiters and observers](../../extending/rate-limiting.md) for
implementation skeletons and trust obligations.

## Failure modes

Important proofs include:

- invalid or first-excess plan structure before attempt effects;
- a preparation key limit of zero, the exact boundary, and above-hard-cap
  client construction;
- global, host, and ordered multi-part key separation;
- cancellation or timeout while waiting for admission or observation;
- an authentication recovery obtaining a second permit;
- both limiter phases seeing the exact initial/recovery `AttemptInfo` values;
- response observation failure preventing later hooks and body work;
- sequential pagination pages sharing the selected limiter correctly;
- local bucket/cooldown capacity failure with no partial state;
- paused-time refill, cooldown clamping, and safe idle pruning;
- poisoned application synchronization reducing to a safe recoverable error;
- no-op and missing-implementation behavior remaining distinct.

## Change hazards

Moving admission after materialization exposes secrets before policy approval.
Moving response observation after hooks or body work makes cooldown state race
with subsequent requests and changes which failure owns the operation.

Compiler changes to key dimensions require semantic validation against final
endpoint shaping, deterministic ID generation, generated code, runtime plan
construction, UI diagnostics, and Rust 1.85 semantic checks. Do not solve a
new rule form by making `RateLimitPlan` unbounded or debuggable.

A new concrete algorithm belongs in `concord-extras` when the existing public
traits are sufficient. A required new lifecycle privilege or context field is
a core contract change and must be reviewed for redaction, cardinality, and
compatibility.

## Verification

Primary owners are:

- `crates/concord-core/src/rate_limit/` for plan construction, limiter
  contracts, and safe errors;
- `crates/concord-core/src/runtime/config.rs` for limiter snapshot and
  preparation-limit validation;
- `crates/concord-core/src/execute/tests/ordering.rs` for admission and
  response-observation order;
- `crates/concord-core/tests/public_api/rate_limit_api.rs` and rate-limit core
  UI cases for the stable semantic surface and privacy;
- `crates/concord-extras/src/rate_limit/local/tests.rs` for quota, cooldown,
  capacity, pruning, atomicity, sharing, and paused-time behavior;
- `crates/concord/tests/generated/rate_limits.rs` and the façade rate-limit UI
  families for generated plans and diagnostics;
- `crates/concord/tests/extensions/rate_limit_composition.rs` and
  `crates/concord-examples/src/runtime_policy/` for downstream composition.

Run focused core and extras owners, then all-feature workspace tests, UI,
warning-denied no-default/selected/all-feature Rustdoc, and the Rust 1.85
semantic checks in [Testing strategy](../testing.md).

## Related

- [Per-host and per-account rate limits](../../recipes/rate-limit-partitioning.md)
- [Extension contracts](extensions.md)
- [Feature composition and evolution](../architecture/feature-composition.md)
- [Managed executor](../architecture/managed-executor.md)
