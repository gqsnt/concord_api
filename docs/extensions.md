# Extensions

- **Audience:** semantic extension and runtime-configuration authors
- **Purpose:** guide
- **Normative references:** [runtime model](runtime.md), [design invariants](design_invariants.md)

Concord extends semantic roles while core retains native transport, ordering,
limits, and information-flow boundaries.

## Extension families

| Trait family | Responsibility |
|---|---|
| `EncodeBody`, `DecodeBody` | complete typed buffered encoding and decoding |
| `RequestBodySpec`, `ResponseBodySpec` | request input lifecycle and response delivery mode |
| `RecordDecoder` | incremental bounded record framing and decoding |
| `CredentialProvider` | bounded credential acquisition and recovery |
| `PaginationController` | deterministic page-state transitions |
| `RateLimiter`, `RateLimitObserver` | policy admission and sanitized response observation |
| `RetryClassifier` | safe retry decisions |
| `RuntimeHooks` | sanitized lifecycle observation |

## Custom response modes remain core-owned

<!-- concord-source: crates/concord-examples/src/request_response/specifications.rs#response_body_spec_extension -->
```rust
pub struct ExampleBufferedSpec;

impl ResponseBodySpec for ExampleBufferedSpec {
    type Output = usize;
    type Mode = Buffered;

    fn from_body(body: Bytes, _head: &ResponseHead<'_>) -> Result<usize, ResponseBodyError> {
        Ok(body.len())
    }
}

pub struct ExampleStreamingSpec;

impl ResponseBodySpec for ExampleStreamingSpec {
    type Output = ByteStream;
    type Mode = Streaming;

    fn from_body(
        body: ByteStream,
        _head: &ResponseHead<'_>,
    ) -> Result<ByteStream, ResponseBodyError> {
        Ok(body)
    }
}

pub struct ExampleDiscardSpec;

impl ResponseBodySpec for ExampleDiscardSpec {
    type Output = ();
    type Mode = Discard;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        Ok(())
    }
}
```

Custom response specifications select one sealed mode—buffered, streaming, or
discard. Core owns native response acquisition, byte/non-progress limits,
cancellation, status ordering, and completion. The extension owns semantic
acceptance and conversion within its mode.

## HTTP configuration

Generated builders expose `configure_http` for transport and deployment
choices such as HTTPS-only policy, protocol selection, proxy, roots,
application identity PEM, connection behavior, and content decoding.

Application and provider HTTP clients are separate. Both may use custom roots;
only the application client uses application identity PEM.

## Runtime configuration

`configure_runtime` selects hooks, limiter, finite limits, debug, and related
runtime components. Setters replace one selected component or limit. Repeated
closures apply immediately in call order before `build()` freezes the snapshot.

Retry has its own generated builder method because it is a coherent runtime
policy with shared rolling-budget state.

## Trust boundary

Extensions are ordinary application Rust and are trusted with the data they
receive. They must own:

- finite internal execution and state;
- safe public error messages;
- logging and storage policy;
- external I/O behavior permitted by the trait;
- no hidden retry loops or detached correctness work.

Core bounds its own retained data and supplies finite context, but does not
sandbox custom Rust.

## What extensions do not receive

No extension receives an unrestricted native application request or response.
Providers receive isolated HTTP rather than the application client. Body and
response specs receive semantic bounded data. Hooks receive sanitized events.
Limiters receive rate plans and key values but no credentials or body.

## First-excess and progress rules

Extensions honor context limits. Known excess fails before retention. Unknown
excess withholds the first excess unit. Request streams, response streams, and
record decoders must make finite progress.

## Crate ownership

- `concord-core` owns semantic traits and the managed executor.
- `concord-extras` owns optional first-party implementations.
- `concord` re-exports the supported application surface and composes enabled
  defaults.

Applications depend on `concord`.

## Design question for a new extension

Ask whether the use case can be represented as a semantic role with bounded
input and output. If it requires unrestricted native request mutation or a
second execution path, it conflicts with the core product model and should be a
separate lower-level integration rather than a Concord extension.

## Related

- [Request bodies](request_bodies.md)
- [Responses](responses.md)
- [Authentication](auth.md)
- [Rate limits](rate_limits.md)
- [Runtime endpoints](runtime_endpoints.md)
