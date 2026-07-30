# Runtime model

Every generated direct request, explicit generated endpoint, runtime-defined
endpoint, pagination page, and response terminal uses one managed executor.

Request construction, scope selection, endpoint construction, and planning are
inert with respect to HTTP, hooks, credential providers, limiters, body
factories, and completion.

## Client snapshot

`build()` validates the fixed base, required values, protected material, HTTP
configuration, runtime configuration, and limiter selection. The result is an
immutable client snapshot.

Client clones share the selected transport, hooks, retry policy, credential
slots, limiter, and runtime limits. Independent builds own independent
components unless the application supplies a shared extension object.

## Planning

`client.plan(&endpoint)` executes the real public head-shaping path and returns
a `SafeRequestPlan`.

Planning validates the endpoint identity, method, logical URL, public headers,
body shape, timeout, and finite structural budgets. It does not execute hooks,
encode or poll a body, acquire credentials or limiter authority, construct a
native request, perform HTTP, or dispatch completion.

Planning returns a credential-free logical URL. It is not an authenticated
request preview.

## Execution sequence

Polling a request terminal starts one operation:

1. Resolve and validate the public method, logical URL, public headers,
   endpoint identity, timeout, and finite budgets.
2. Inspect bounded request-body metadata without materializing body bytes.
3. Run the before-send hook with safe public operation data.
4. Prepare or reconstruct the managed request body.
5. Resolve current credential material without placing it.
6. Acquire every resolved rate-limit policy in source order.
7. Place private authentication material and materialize the native request.
8. Perform HTTP under the selected operation deadline.
9. Present the sanitized response head to the limiter observer.
10. Present the sanitized response head to the response hook.
11. Apply authentication recovery or an admitted retry decision.
12. Apply declared status acceptance.
13. Acquire buffered, streaming, or discard response data.
14. Dispatch one completion event after the final outcome is fixed.
15. Deliver the final result.

Limiter admission precedes private authentication placement. Limiter response
observation precedes the user response hook. Both response observers precede
status acceptance and body acquisition.

## Attempts and recovery

One operation has one operation identity, one deadline selection, one final
result, and one completion event.

An admitted retry repeats before-send, body reconstruction, credential
preflight, limiter admission, private placement, transport, and response-head
observation. The retry policy bounds per-operation attempts and aggregate
extra load.

Authentication recovery performs at most one admitted recovery for the whole
operation. Endpoint-backed acquisition runs its source endpoint through the
same executor. Provider acquisition uses isolated bounded provider HTTP.

Known-safe transport protocol recovery may occur inside one visible attempt.
It does not create another hook, limiter, or retry-policy attempt.

## Deadlines

The operation timeout covers preparation, hooks, credential acquisition,
limiter waiting, transport, response-head observation, and complete buffered
acquisition and decoding.

Completion observes an already fixed result and runs outside the operation
timeout. Completion is awaited and is not detached.

An accepted streaming response returns after the stream is opened. Later byte
or record polling is consumer-owned and follows stream progress and byte
limits rather than the completed operation timeout.

## Response modes

`ResponseBodySpec` selects one sealed mode:

| Mode | Runtime result |
|---|---|
| `Buffered` | acquire a complete bounded body before conversion |
| `Streaming` | return a bounded lazy stream |
| `Discard` | discard the response body under finite drain rules |

Status acceptance runs before a body is delivered as success. A rejected
status may retain one complete bounded `StatusBody`; first excess discards the
body and records only its structural state.

`response_bytes()` preserves the response specification's status acceptance
and `Accept` header, skips declared decoding, and returns complete bounded
bytes with accepted metadata. It is callable only for buffered or discard
response modes.

## Pagination

Pagination constructs sequential page requests through the same executor.
Each page has a child operation identity and repeats hooks, credentials,
limiter admission, HTTP, response-head observation, and body acquisition.

The controller advances only declared live bindings. Cycle detection uses a
canonical public request identity. Page count, item count, retained identity
bytes, and call-site limits are independent.

Pagination is all-or-error unless the caller selects a finite `PageLimit::Take`
prefix.

## Cancellation and progress

Dropping a request future cancels remaining foreground work. Concord does not
detach hooks, completion, or correctness work.

Managed request and response streams suppress empty chunks and bound
non-progress. Record decoders have bounded staging and consecutive
non-progress limits.

Successfully yielded bytes or records belong to the application. A later
stream error does not retract them.

## Observation and redaction

`EndpointDescription` contains structural endpoint facts and no bound values,
bodies, credentials, resolved key values, or runtime object.

`SafeRequestPlan`, hooks, retry classifiers, limiter observers, completion,
and errors receive purpose-specific safe structures. The logical URL and
public headers may contain application data even though credentials are
absent.

`ApiError` and `ClientBuildError` retain stable categories and safe kinds.
Native parser, transport, provider, codec, body, and hook causes are reduced or
dropped before they cross the public boundary.

Actual accepted response headers and decoded output are application data.
Explicit accessors return them; automatic `Debug` implementations omit their
values.

## Runtime configuration

The generated builder applies `retry`, `configure_http`, and
`configure_runtime` transformations immediately in call order.

`HttpConfig` owns HTTPS policy, protocol selection, proxy, root certificates,
application identity PEM, connection behavior, and content-encoding
selection.

`RuntimeConfig` owns finite request, response, stream, pagination,
authentication, rate-limit, hook, and completion settings. Positive limits
use nonzero types where zero has no semantic meaning.

An explicitly configured limiter takes precedence over the façade-provided
default limiter.

## Related

- [DSL specification](spec.md)
- [Bodies and responses](bodies_and_responses.md)
- [Authentication](auth.md)
- [Retry and rate limiting](retry_and_rate_limit.md)
- [Customization](customization.md)
