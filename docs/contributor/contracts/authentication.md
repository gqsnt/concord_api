# Authentication contract

Authentication is a managed capability, not a request-header convenience. Core
owns credential slots, provider coordination, placement, challenge attribution,
and the single permitted recovery path. Generated code and runtime-defined
endpoints describe requirements; neither receives protected material or sends
authenticated HTTP independently.

## Purpose

The contract separates three responsibilities:

- declarations and endpoint builders select named authentication requirements
  and public placement metadata;
- credential implementations acquire, retain, and expose protected material
  through semantic traits;
- the managed executor decides when material may be requested, attached,
  invalidated, refreshed, and retried.

This separation lets planning and hooks inspect the logical request without
seeing credentials. It also keeps authentication attempts under the same
deadline, limiter, hook, error, and completion rules as every other request.

## Public model

`concord::auth` exposes the stable vocabulary:

- `CredentialMaterial` describes shareable material and optional monotonic
  expiration;
- `SecretCredential` explicitly exposes single-string material only at the
  native authentication boundary;
- `CredentialProvider` supplies `acquire`, `refresh`, and `recovery_mode`;
- `CredentialRequest` carries a finite, isolated `ProviderHttp` capability and
  a structural `CredentialReason`;
- `CredentialHandle` and `CredentialRef` connect a built client to its
  credential slots;
- `AccessToken`, `ApiKey`, `BasicCredential`, and `SecretString` are bounded
  built-in material types.

Placement is a separate concern. Bearer, Basic, exact/static-scheme
Authorization, named-header, and query placement describe how compatible
material is serialized after policy approval. A provider does not choose its
endpoint placement, and a declaration does not gain access to provider state.

The exact application workflow belongs to
[Authentication](../../features/authentication.md), while implementation
skeletons belong to
[Credentials and providers](../../extending/credentials-and-providers.md).

## Internal flow

One visible attempt follows this authentication sequence:

1. public request shaping reserves and validates protected placement locations;
2. `before_send` sees the credential-sanitized logical head;
3. request-body preparation establishes the managed body;
4. authentication preflight resolves every required slot and provider flight;
5. the rate limiter admits the still-sanitized attempt;
6. exact-current material is checked again, formatted, and attached at the
   native boundary;
7. the request is sent;
8. a supported final challenge may be attributed to exact material used by the
   attempt;
9. the provider's recovery mode decides whether that material is retained,
   invalidated, or refreshed before at most one application-level replay.

Late materialization matters. A limiter rejection or timeout cannot leave a
partially authenticated native request, and waiting behind a concurrent
provider flight does not authorize use of material that has since changed.

Provider HTTP is an isolated managed path for credential acquisition. It has
its own request and response bounds and does not inherit the protected
endpoint's authentication, limiter plan, hooks, or native client escape
capabilities.

The complete lifecycle position is documented in
[Managed executor](../architecture/managed-executor.md).

## Architectural invariants

- Protected values are absent from the public request head, planning output,
  lifecycle hooks, built-in debug events, limiter input, and ordinary error
  formatting.
- A credential slot publishes immutable material. Overlapping operations may
  share it, but replacement and invalidation are coordinated by the slot.
- Acquisition and refresh for one slot are coalesced. Waiters re-evaluate the
  slot after the in-flight provider call finishes.
- Expiration, proactive refresh, and challenge recovery compare exact-current
  material rather than invalidating a newer value produced by another
  operation.
- One operation gets at most one Concord-owned authentication recovery replay
  across all policy attempts. Private protocol recovery does not call the
  provider or body factory.
- Every replay is another visible attempt: it passes attempt hooks, limiter
  admission, materialization, and response observation. The enclosing request
  or page still produces one fixed completion event.
- The initial attempt reports `AttemptInfo { index: 0, policy_retry_index: 0,
  authentication_recovery: false }`. Authentication recovery increments the
  current global attempt index, retains the number of policy retries already
  started, and sets `authentication_recovery: true`.
- A successful ordinary result reports one started attempt and zero policy
  retries in `ExecutionMeta`. Policy retries and the one authentication
  recovery contribute independently to the final checked totals. Completion
  uses those same totals without emitting an intermediate event.
- Authentication errors cross the public boundary only after reduction to safe
  categories and finite structural context.
- Bearer, Basic, exact, and static-scheme Authorization placements share one
  exclusive target. Scheme metadata is a bounded case-preserving HTTP token;
  the final value is rendered with checked arithmetic and exactly one space
  only after admission.

These are semantic invariants. The current slot types and helper-function
layout are implementation details and may change without changing the
contract.

## Bounds

Core bounds retained secret strings, built-in credential fields, placement
counts and names, static Authorization schemes, formatted placement values,
provider HTTP bodies, refresh skew, recovery count, and provider-failure
cooldown state. The canonical values and configuration points are in the
[limits reference](../../reference/limits.md).

Those core bounds do not make an arbitrary provider finite. A provider owns
the size and cardinality of its caches, queues, metrics, background tasks, and
external calls. Its future runs under the remaining operation deadline and is
dropped on cancellation, but synchronous work before returning that future
cannot be preempted. With no operation timeout, provider liveness is trusted.

## Errors and redaction

Provider and material failures reduce to authentication-focused
`ErrorCategory` and `AuthErrorKind` values. The original provider error text is
not carried into `ApiError`. Credential constructors similarly expose
value-free error kinds rather than rejected secret bytes.

`SecretString` redacts both `Display` and `Debug`, but that wrapper is not an
automatic memory-erasure or logging policy. Custom material and provider
implementations remain responsible for storage, destruction, and every
explicit call to `expose_secret`.

A challenge is accepted for recovery only when its scheme and placement can be
attributed to exact material used by the attempt. An unrelated `401` remains
an ordinary HTTP status failure. See
[Errors](../../reference/errors.md) and
[Security and redaction](../../concepts/security-and-redaction.md) for the
canonical public reduction rules.

## Extension boundary

A custom authentication extension may implement `CredentialMaterial`,
`SecretCredential`, or `CredentialProvider`. It receives no native request,
native response, protected endpoint head, limiter object, or hook state.
Provider HTTP is the only transport capability supplied to a provider.

Calls for independent operations may overlap. Implementations must protect
their inputs, avoid holding application locks across external I/O, and bound
work that Concord cannot measure. Recovery behavior must be selected through
`RecoveryMode`; an extension must not create its own hidden replay loop.

Exact/static-scheme Authorization requires `SecretCredential` and therefore
accepts built-in API-key or Bearer material, compatible endpoint output, or
compatible custom-provider output. Basic material is deliberately
incompatible. This placement capability does not expose request-signing
inputs.

First-party OAuth2 support in `concord-extras` uses the same public contracts.
That downstream implementation is evidence that the trait boundary is
sufficient, not a privileged path.

## Failure modes

Important terminal cases include:

- missing, expired, or proactively refreshed material cannot be acquired;
- constructed material or a formatted placement exceeds its finite bound;
- a provider future fails, times out, or leaves a slot in cooldown;
- a required placement conflicts with public request shaping;
- exact material used by an attempt is rejected and recovery is disabled or
  exhausted;
- a one-shot request body cannot be replayed after an attributable challenge;
- cancellation drops provider or executor futures after already-completed
  external effects.

Each case must retain the normal lifecycle ordering and emit at most one
completion event for the enclosing request or page, not one per recovery
attempt. Pagination aggregate completion sums the page attempt and policy
retry totals with checked arithmetic.

## Change hazards

Review authentication changes for accidental early exposure, lost
exact-current comparisons, duplicate provider flights, unbounded recovery,
and replay paths that bypass hooks or limiter admission. Adding a placement
requires compiler validation, public planning behavior, runtime
materialization, redaction, conflict detection, generated UI coverage, and
provider-independent tests to evolve together.

Changes to the private generated/runtime handshake follow
[Generated contract versioning](../generated-contract.md). A new reusable
provider belongs in `concord-extras`; a new lifecycle privilege requires a
core contract review rather than an extras-only shortcut.

## Verification

The main proof owners are:

- `crates/concord-core/src/auth/tests.rs` and
  `crates/concord-core/src/auth/slot/tests.rs` for material and slot
  transitions;
- `crates/concord-core/src/execute/auth_tests/` for preflight,
  materialization, provider coordination, and recovery sequencing;
- `crates/concord-core/tests/security/` for error and hook redaction;
- `crates/concord-core/tests/lifecycle/https_auth_contract.rs` for the real
  HTTPS boundary;
- `crates/concord/tests/generated/authentication.rs` and the façade
  authentication UI cases for generated binding and diagnostics;
- `crates/concord/tests/extensions/oauth2.rs` and
  `crates/concord-examples/src/authentication/` for downstream/provider usage.

Use focused tests while editing, then run the workspace all-feature suite,
warning-denied Rustdoc, UI owners, and the Rust 1.86.0 semantic checks described
in [Testing strategy](../testing.md).

## Related

- [Credential providers and OAuth2](../../features/credential-providers.md)
- [Extension contracts](extensions.md)
- [Finite behavior](../../concepts/finite-behavior.md)
- [Compile-time to runtime flow](../architecture/compile-runtime-flow.md)
