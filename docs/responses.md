# Responses

- **Audience:** API authors, callers, and response-extension authors
- **Purpose:** concept guide
- **Normative references:** [DSL request/response rules](spec.md#request-and-response-specifications), [runtime model](runtime.md)

Do not conflate the response specification, native HTTP response, accepted
head, acquisition mode, decoded output, metadata wrapper, raw bytes, error
body, stream, or paginated page output.

## Declaration

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#response_acquisition_contract -->
```rust
api! {
    client ResponseApi {
        base "http://responses.example.test/";
    }

    GET message() -> Text;
    GET optional_message() -> crate::request_response::acquisition::OptionalText;
}
```

The return type implements `ResponseBodySpec`. Its associated output is what a
direct `.await` returns. The response specification also selects status
acceptance, `Accept` semantics, a sealed acquisition mode, and conversion.

## Three acquisition modes

| Mode | Meaning |
|---|---|
| Buffered | acquire a complete bounded body before conversion |
| Streaming | return a bounded lazy stream after the response opens |
| Discard | intentionally discard the accepted body under finite rules |

`Bytes`, `Text`, and `Json<T>` are buffered. `Stream` and `Records<D>` are
streaming. `NoContent` uses discard mode.

## Caller terminals

Direct decoded output:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#decoded_response -->
```rust
/// Returns only the decoded application value.
pub async fn decoded(client: &ResponseApi) -> Result<String, ApiError> {
    client.message().await
}
```

Accepted metadata plus output:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#metadata_response -->
```rust
/// Returns the decoded value together with accepted response metadata.
pub async fn with_metadata(client: &ResponseApi) -> Result<ApiResponse<String>, ApiError> {
    client.message().response().await
}
```

Accepted bounded bytes without declared conversion:

<!-- concord-source: crates/concord-examples/src/request_response/acquisition.rs#response_bytes -->
```rust
/// Preserves declared acceptance and `Accept` while returning bounded bytes.
pub async fn as_bytes(client: &ResponseApi) -> Result<ApiResponse<Bytes>, ApiError> {
    client.message().response_bytes().await
}
```

`.response_bytes()` preserves the declared status acceptance and `Accept`
header and exists only for buffered or discard modes. Pagination is available
only for buffered page-capable response specifications.

## Status, challenge, retry, and body order

The response head is observed by the limiter and user response hook before
status acceptance and body acquisition. Authentication challenge/recovery and
admitted retry decisions occur before declared status acceptance. This prevents
an application response specification from accepting a 401 before the
credential policy can act on it.

## Buffered responses

A buffered response is fully acquired under the operation deadline and finite
response limit. Known excessive lengths fail before collection. Unknown
lengths withhold and discard the first excess chunk. Conversion runs only after
successful bounded acquisition.

## Streaming responses

Opening an accepted stream completes the request operation. Later polling is
consumer-owned and follows cumulative byte and finite non-progress limits.
Already yielded bytes or records are not retracted by a later stream error.

Streaming responses are not page-capable because a page must be completely
decoded before the controller can produce the next request and before an
aggregate result can be defined.

## Records

`BufferedRecords<D>` collects records inside the operation. `Records<D>`
returns a lazy record stream. `Csv<T>` and `NdJson<T>` are complete buffered
response specifications; `Records<Csv<T>>` and `Records<NdJson<T>>` stream the
same framing.

Source bytes, staging, record size, item count, and decoder non-progress have
independent limits.

## Rejected statuses and errors

An unaccepted status produces `HttpResponseFailure` and may retain one complete
bounded `StatusBody`. Automatic formatting exposes structural state and length;
explicit accessors expose retained bytes.

Public errors preserve stable categories while native parser, transport,
provider, codec, body, and hook causes are reduced or dropped before crossing
the boundary.

## Metadata and sensitive application data

Actual accepted headers and decoded output are application data. Explicit
accessors return them. Automatic `Debug` omits their values. Credential-free
does not imply universally safe to log.

## Extension boundary

A custom `ResponseBodySpec` receives a semantic accepted head and a core-owned
bounded mode body. It never receives the native transport response. Core keeps
ownership of acquisition, deadlines, limits, cancellation, and completion.

## Related

- [Request bodies](request_bodies.md)
- [Pagination](pagination.md)
- [Planning and observation](planning_and_observation.md)
- [Extensions](extensions.md)
