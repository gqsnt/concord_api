# Extension model

Extend Concord through the smallest public semantic contract that owns the
behavior you need. An extension receives typed values and bounded context; it
does not receive a Reqwest request, a native response, a transport client, or
access to generated internals.

This boundary keeps request shaping, credentials, limits, status handling,
completion, and transport ownership in Concord while allowing application
code to supply domain-specific behavior.

## Choose the trait family

Start with the operation your extension performs:

| Need | Public contract | Use it when |
|---|---|---|
| Encode one complete buffered request | `concord::codec::EncodeBody` | A synchronous value-to-bytes conversion is sufficient |
| Decode one complete buffered response | `concord::codec::DecodeBody` | Bounded bytes can be converted synchronously |
| Define request metadata and preparation | `concord::body::RequestBodySpec` | The body is streamed, factory-backed, multipart, or needs structural metadata |
| Select a response acquisition mode | `concord::response::ResponseBodySpec` | The result needs buffered, streaming, or discard behavior beyond a codec |
| Frame a response into records | `concord::records::RecordDecoder` | One bounded byte stream should yield typed records incrementally |
| Acquire renewable credentials | `concord::auth::CredentialProvider` | Credential material comes from a bounded asynchronous provider workflow |
| Derive the next page state | `concord::pagination::PaginationController` | Offset, page-number, or cursor helpers cannot express the service |
| Own admission and quota state | `concord::rate_limit::RateLimiter` | Local or distributed policy must authorize each attempt |
| Interpret response cooldowns | `concord::rate_limit::RateLimitObserver` | A limiter needs service-specific response-head policy |
| Observe or veto lifecycle points | `concord::hooks::RuntimeHooks` | Bounded application telemetry or policy belongs at stable lifecycle events |

Use the first matching row. Do not implement a full body specification merely
to rename a media type, and do not implement a lifecycle hook to reproduce a
codec or limiter.

## Buffered codecs or body specifications

The body extension choice is a two-by-two matrix:

| Direction | Complete buffered value | Full lifecycle |
|---|---|---|
| Request | `EncodeBody` | `RequestBodySpec` |
| Response | `DecodeBody` | `ResponseBodySpec` |

`EncodeBody` and `DecodeBody` receive finite semantic contexts and complete
`Bytes`. Concord supplies blanket implementations of the corresponding body
specifications. This is the normal extension path for a custom serialization
format.

Implement `RequestBodySpec` directly when metadata must be available before a
hook runs or when preparation returns a managed stream, factory, or multipart
body. Implement `ResponseBodySpec` directly to choose one of Concord's sealed
`Buffered`, `Streaming`, or `Discard` modes. Applications can select those
modes but cannot add an acquisition mode or obtain the underlying response.

Record decoding is a separate incremental contract layered on a bounded byte
stream. It is not a fourth response acquisition mode.

## How types enter a declaration

The declaration compiler treats extension types as ordinary Rust paths. A
request body, response specification, credential provider, or pagination
controller is checked by Rust against its public trait at the generated use
site. The compiler does not recognize concrete helper names or give first-party
implementations privileged behavior.

Generated and runtime-defined endpoints converge on the same body and response
contracts. A custom implementation therefore does not need a second adapter
for `RuntimeEndpoint`.

Install runtime-wide objects through the generated client's
`configure_runtime` builder method. The focused rate-limiter and hooks pages
include compiler-owned construction examples.

The objects are shared by clones of that built client. Independently built
clients share state only when the application deliberately gives them the same
`Arc`.

## The boundary Concord retains

Extensions do not own URL authentication, native request mutation, redirects,
transport execution, status reduction, response acquisition, operation IDs,
or completion dispatch. A provider gets the bounded `ProviderHttp` capability
instead of the application transport. A limiter gets an immutable semantic
plan instead of a request handle. Hooks get credential-sanitized events rather
than bodies or authenticated headers.

“Sanitized” does not mean harmless. Public paths, queries, headers, hosts,
records, and decoded values may still contain customer or operational data.
Each extension owns its logging, metrics, storage, and retention decisions.

## Finite work and safe failures

Public extension contexts expose only the finite inputs relevant to the
contract: configured byte ceilings, structural metadata, a response head, a
pagination state, or a bounded provider HTTP capability. These contexts do not
automatically bound application-created maps, queues, retries, tasks, output
objects, or metric cardinality.

Most extension errors retain a stable category and, where supported, explicitly
reviewed safe text. Do not attach parser messages, bodies, credentials, cursor
values, resolved quota keys, or native errors to them. Dropping a source error
before constructing `CodecError`, `RecordError`, `AuthError`,
`PaginationError`, or `RateLimitError` is intentional.

Synchronous callbacks cannot be preempted. Asynchronous provider, limiter, and
result-determining hook futures normally run under the remaining operation
deadline, while completion hooks run after the result is fixed and outside
that deadline. Cancellation stops later Concord work but cannot undo extension
side effects that already happened. The
[hooks, trusted work, and bounds](hooks-and-trusted-work.md) page owns the
cross-cutting liveness contract.

## Rustdoc map

Use the façade paths in application code and Rustdoc:

- body codecs: `concord::codec::EncodeBody` and
  `concord::codec::DecodeBody`;
- body lifecycle: `concord::body::RequestBodySpec` and
  `concord::response::ResponseBodySpec`;
- records: `concord::records::RecordDecoder`;
- credentials: `concord::auth::CredentialProvider` and
  `concord::auth::CredentialMaterial`;
- pagination: `concord::pagination::PaginationController`;
- rate policy: `concord::rate_limit::RateLimiter` and
  `concord::rate_limit::RateLimitObserver`;
- lifecycle: `concord::hooks::RuntimeHooks`.

Ordinary application crates should not depend on `concord-core` or
`concord-extras` to reach these contracts.

## Related

- [Codecs](codecs.md)
- [Request and response body specifications](body-specifications.md)
- [Credentials and providers](credentials-and-providers.md)
- [Hooks, trusted work, and bounds](hooks-and-trusted-work.md)
- [Generated API reference](../reference/generated-api.md)
- Rustdoc paths: `concord::codec`, `concord::body`,
  `concord::response`, and the other façade modules listed above
