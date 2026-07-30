# Customization

Concord exposes semantic extension traits. An extension receives typed bounded
data and never receives an unrestricted native application request or
response.

## Extension families

| Trait family | Responsibility |
|---|---|
| `EncodeBody`, `DecodeBody` | complete typed encoding and decoding |
| `RequestBodySpec`, `ResponseBodySpec` | request input and response delivery mode |
| `RecordDecoder` | incremental record framing and decoding |
| `CredentialProvider` | bounded credential acquisition |
| `PaginationController` | deterministic page-state transitions |
| `RateLimiter`, `RateLimitObserver` | policy admission and sanitized response observation |
| `RetryClassifier` | safe retry decisions |
| `RuntimeHooks` | sanitized lifecycle observation |

## HTTP configuration

Generated builders expose:

```rust
let client = ServiceApi::builder()
    .configure_http(|http| {
        http
            .https_only(true)
            .identity_pem(identity_pem)
    })
    .build()?;
```

`HttpConfig` validates proxy structure, certificate counts and sizes,
application identity size, protocol selection, and transport capabilities
before the client snapshot is created.

The application HTTP client and provider HTTP client are separate. Both use
custom roots; only the application client uses application identity PEM.

## Runtime configuration

```rust
let client = ServiceApi::builder()
    .configure_runtime(|runtime| {
        runtime
            .hooks(hooks)
            .rate_limiter(limiter)
    })
    .build()?;
```

Runtime setters replace one selected component or finite limit. Repeated
configuration closures apply immediately in call order.

## Hooks

`RuntimeHooks` receives `BeforeSend`, `ResponseHeadEvent`, and completion
events. Events contain safe operation identity, logical URL or sanitized
headers where required, status, timing, attempt counts, and structural body
facts.

Hooks do not receive credentials, private authentication query values, body
bytes, native requests, native responses, or native error chains.

Hook rejection stops the current stage. Completion observes one already fixed
outcome.

Hook implementations are trusted application code. They own finite execution,
safe messages, and logging policy.

## Error contract

Extension failures cross the public boundary only through stable safe error
types or application-approved safe messages.

Body content, credential material, URLs, header values, cursors, rate-limit
key values, parser output, and backend diagnostics are not safe messages.

## Bounds

Extensions honor the context limits supplied by the runtime. They do not add
unbounded buffering, retry loops, detached work, or hidden native transport.

First excess is withheld where a collection crosses a hard ceiling.
Non-progress is finite for request streams, response streams, and record
decoders.

## Crate ownership

`concord-core` owns semantic traits and the managed executor.
`concord-extras` owns first-party implementations.
`concord` re-exports the supported application surface and composes enabled
defaults.

An application depends on `concord`.

## Related

- [Runtime model](runtime.md)
- [Authentication](auth.md)
- [Retry and rate limiting](retry_and_rate_limit.md)
- [Design invariants](design_invariants.md)
