# Errors, redaction, hooks, and completion contract

Every request path eventually produces either a typed value or one safe
structured failure. Hooks and built-in debug output observe sanitized
lifecycle state, and completion observes an already-fixed result. These
contracts are designed together: a new terminal path is incomplete until its
error reduction, observation ordering, and completion outcome are defined.

## Purpose

The boundary has three jobs:

- preserve useful stable structure without forwarding native or
  extension-owned error chains;
- expose only reviewed request and response metadata to observers;
- make success and failure converge on one completion path without letting
  completion rewrite the result.

The exhaustive application-facing categories belong to
[Errors](../../reference/errors.md). This page explains how maintainers
preserve them across the executor.

## Public model

`ApiError` exposes:

- a non-exhaustive top-level `ErrorCategory`;
- stable endpoint identity and application method;
- focused configuration, authentication, rate-limit, and pagination kinds
  when applicable;
- final HTTP status, sanitized retained headers, safe `Retry-After`, and an
  optional bounded `HttpResponseFailure`.

`RuntimeHooks` exposes four lifecycle callbacks:

- `before_send` can reject an attempt before body materialization and native
  execution;
- `on_response_head` can reject after limiter observation and before
  challenge, status, or body handling;
- `on_complete` observes each already-fixed direct request or page result;
- `on_pagination_complete` observes the already-fixed aggregate pagination
  result.

Events carry operation correlation, endpoint, method, credential-sanitized
logical URL, page structure, finite `AttemptInfo`, timing/status facts, and
`CompletionOutcome`. `AttemptInfo` separates the visible-attempt index,
policy-retry index, and authentication-recovery flag. Header-bearing events
use sanitized views.

Built-in debug levels use the same semantic event boundary and never expose
body bytes or native request/response objects.

`ClientBuildError` is the separate pre-execution construction contract. Its
non-exhaustive kind distinguishes missing input, invalid credential, base URL,
HTTPS policy, unavailable TLS, proxy installation, root certificate, client
identity, final native client build, runtime configuration, unavailable
limiter, and internal failure. Only generated input metadata is optional;
transport and TLS kinds remain value-free and source-less.

## Internal flow

Core reduces failures at the stage that owns them. Configuration, request
encoding/body, authentication, rate limiting, hook veto, deadline, transport,
status, response acquisition/decoding/contract, pagination, and internal
invariant failures each map to one stable category. Focused kinds add detail
only for reviewed finite subcontracts.

Request observation occurs after the public head and structural body metadata
are valid but before body preparation, credential attachment, limiter
acquisition, or transport. Response observation occurs in this order:

1. the native final response head is received;
2. built-in debug observes sanitized response structure;
3. limiter response observation completes;
4. `on_response_head` observes sanitized structure;
5. authentication challenge handling runs;
6. declared status acceptance selects accepted body acquisition or rejected
   status capture.

Once the terminal `Result` is fixed, core derives `CompletionOutcome`, records
final status, elapsed executor time, started-attempt count, policy-retry count,
and authentication-recovery summary, emits built-in completion observation,
and awaits `on_complete`. That callback runs outside the operation deadline.
It delays delivery but cannot modify the fixed value or error. Intermediate
attempts do not emit completion.

Pagination completes each page through that path before invoking one
aggregate completion callback. The aggregate event records pages completed,
items retained, checked sums of page attempt/retry totals, recovery summary,
final status, and its own fixed outcome.

## Architectural invariants

- `Display`, `Debug`, and `Error::source` must not expose native errors,
  credentials, body bytes, protected URLs, cursor values, or resolved
  rate-limit keys.
- Public errors contain bounded owned context. Borrowed extension or native
  state cannot outlive reduction.
- Header sanitization applies built-in sensitive-name rules and
  declaration-selected protected names while preserving public repeated-value
  structure.
- Credential-sanitized URLs can still contain public identifiers; callers must
  not interpret “sanitized” as anonymous.
- Result-determining hooks run under the remaining operation deadline.
- Completion runs after the result is fixed, outside that deadline, exactly
  once for every executor result that reaches delivery.
- The initial attempt is `{ index: 0, policy_retry_index: 0,
  authentication_recovery: false }`. Policy retry increments both the global
  index and policy-retry index. Authentication recovery increments the global
  index, retains the current policy-retry index, and sets its flag.
- Ordinary accepted response metadata reports one attempt and zero policy retries.
  Final request/page and aggregate completion report checked totals for every
  started policy retry and the independently bounded authentication recovery.
- Cancellation while awaiting completion may prevent result delivery and does
  not roll back prior hook effects.
- Streaming success completes as `StreamOpened`. Later stream-item failures do
  not emit another completion or change that outcome.
- A failure in an earlier owner prevents later result-determining owners from
  running, but still converges on completion.

Completion event field layout is a public contract. The helper function that
constructs it is not.

## Bounds

Error contexts, endpoint IDs, methods, sanitized headers, terminal status
bodies, elapsed durations, page counts, and item counts are finite or
structural. Terminal status-body capture is explicitly configurable and
bounded; a known-too-large, first-excess, read-failed, or deadline-expired body
is represented by `StatusBodyState` without retaining a prefix as if it were
complete.

`StatusBody::bytes` and explicit decode operate only on a complete retained
body. Status capture is not a logging channel and is separate from accepted
response decoding. Exact bounds belong to the
[limits reference](../../reference/limits.md).

Hooks remain trusted application work. Core cannot bound custom hook state,
metric cardinality, synchronous work before returning a future, spawned tasks,
or the duration of completion. A completion hook that never resolves prevents
delivery even though the result is already fixed.

## Errors and redaction

Reduction intentionally drops unreviewed text. Native Reqwest errors become a
structural timeout, connect, request-body, or transport category and are then
dropped. Codec, body, record, provider, limiter, pagination, and hook failures
expose only their contract-approved kinds or safe fields.

Construction reduction happens at the owner: fixed-base parsing produces
`InvalidBaseUrl`; HTTPS-only policy produces `HttpsRequired`; missing compiled
TLS produces `TlsUnavailable`; native proxy installation produces
`InvalidProxy`; structural and parsed PEM failures produce
`InvalidRootCertificate` or `InvalidClientIdentity`; and final Reqwest client
creation produces `HttpClientBuild`. Runtime and generated-limiter validation
retain their dedicated kinds. The HTTP configuration builder stores only the
first safe startup failure kind, never the rejected bytes.

`HookError` accepts application-designated safe text so the hook itself can be
diagnosed locally, but executor reduction drops that text from `ApiError`.
Changing this behavior would require a deliberate public confidentiality
review.

Some focused extension errors support explicitly reviewed safe messages. Their
constructors and formatting must continue to exclude secrets, bodies, native
chains, and unbounded context. Tests use unique secret sentinels and inspect
both `Display` and `Debug`; checking only the category is insufficient.

Terminal HTTP failures retain sanitized response headers and the opaque finite
status body. Authentication rejection uses the same terminal response wrapper
with an authentication category only when the challenge was attributable to
material used by the attempt.

An accepted non-2xx response is not a terminal HTTP failure. It uses the
declared response acquisition limit and can still fail as response-body,
response-decoding, or response-contract work. Authentication challenge
handling is authoritative before status acceptance.

## Extension boundary

Hooks receive semantic event structs, not mutable requests, credentials,
bodies, native responses, transport clients, limiter objects, or arbitrary
error sources. `before_send` and `on_response_head` can veto but cannot rewrite
the operation. Completion callbacks return `()` and cannot replace the fixed
result.

Calls may overlap across clients, cloned clients, pages, and authentication
attempts. Implementations own synchronization, cardinality, storage,
confidentiality, and external I/O. They should use operation and parent
operation IDs for correlation rather than reconstructing identity from URLs.

The implementation guide is
[Hooks, trusted work, and bounds](../../extending/hooks-and-trusted-work.md).
Cross-cutting extension obligations are in
[Extension contracts](extensions.md).

## Failure modes

Maintain explicit coverage for:

- every result-determining stage failing before later stages begin;
- hook error text and native error sentinels absent from public formatting;
- built-in and declaration-selected sensitive headers redacted in borrowed and
  owned views;
- safe repeated public headers retaining order and non-UTF-8 values becoming a
  structural marker;
- final response status preserved when a later observer or body stage fails;
- successful, failed, timeout, recovered, streaming, and paginated completion
  outcomes;
- delayed completion not changing a fixed success or error;
- cancellation during result-determining work and during completion;
- terminal status-body disabled, empty, complete, known-too-large,
  first-excess, read-failed, and deadline-expired states.

## Change hazards

Adding an `ErrorCategory` requires mapping every terminal owner,
`CompletionOutcome`, public formatting, Rustdoc, reference documentation, and
exhaustiveness-sensitive tests. Adding a focused kind requires proving why its
extra structure is stable and safe.

Changing `ClientBuildErrorKind` requires one deterministic proof for every
named variant plus public API coverage. Proxy, root PEM, identity PEM, and
native build probes must use unique sentinels and prove absence from
`Display`, `Debug`, and `Error::source`. Never retain a native source merely to
make the kind more descriptive.

A new event field expands what application code can retain. Review its
cardinality, sensitivity, lifetime, ownership cost, and behavior on retries
and pages. Never pass a raw header map or native URL merely because a callback
is “trusted.”

Reordering hooks, limiter observation, challenge handling, status capture, or
completion changes product semantics even if method signatures stay the same.
Treat such changes as architecture work and update the managed-executor trace
and ordering tests together.

## Verification

Primary owners are:

- `crates/concord-core/src/error/` for stable categories, focused kinds,
  formatting, sources, and status-body states;
- `crates/concord-core/src/redaction/` and
  `crates/concord-core/tests/security/` for URL/header sanitization and secret
  sentinels;
- `crates/concord-core/src/hooks/` for event and hook contracts;
- `crates/concord-core/src/execute/tests/ordering.rs` and
  `crates/concord-core/src/execute/tests/completion.rs` for lifecycle order and
  convergence;
- `crates/concord-core/src/execute/tests/cancellation.rs` for deadline and
  completion-delivery behavior;
- `crates/concord-core/src/execute/tests/status.rs` and
  `crates/concord-core/tests/bounds/status_body_contract.rs` for terminal
  response ownership;
- `crates/concord-core/tests/public_api/error_api.rs` and
  `crates/concord-core/tests/public_api/hooks_api.rs` for the stable surface;
- `crates/concord/tests/runtime/lifecycle.rs` for façade-level event
  correlation.

After focused owners, run workspace tests, doctests, warning-denied Rustdoc,
UI/privacy cases, and analyzer checks from
[Testing strategy](../testing.md).

## Related

- [Hooks and observability](../../features/hooks-and-observability.md)
- [Security and redaction](../../concepts/security-and-redaction.md)
- [Finite behavior](../../concepts/finite-behavior.md)
- [Managed executor](../architecture/managed-executor.md)
