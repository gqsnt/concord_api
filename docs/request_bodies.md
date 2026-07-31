# Request bodies

- **Audience:** API authors, callers, and body-extension authors
- **Purpose:** concept guide
- **Normative references:** [DSL request/response rules](spec.md#request-and-response-specifications), [runtime model](runtime.md)

A request body is more than one serialized parameter. Concord separates body
role, caller input, specification type, structural metadata, preparation,
managed material, replay capability, page stability, and finite transport.

## Body role is explicit

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#body_contract -->
```concord
api! {
    client BodyApi {
        base "http://body.example.test/";
    }

    POST json_round_trip(#[body] body: Json<Vec<u32>>) -> Json<Vec<u32>>;
    POST text_round_trip(#[body] body: Text) -> Text;
    POST custom_round_trip(
        #[body] body: crate::request_response::specifications::ExampleCodec
    ) -> crate::request_response::specifications::ExampleCodec;

    POST upload(#[body] body: Stream) -> Bytes;
    GET download() -> Stream;
    POST exact_upload(#[body] body: Stream<OctetStream>) -> Bytes;
    GET exact_download() -> Stream<OctetStream>;

    POST multipart_upload(#[body] body: Multipart) -> NoContent;
}
```

`#[body]` is endpoint-only, accepts no arguments, and may appear at any
parameter position. A parameter without the marker is an ordinary public value
regardless of its name.

Role is not inferred from a name, position, or recognized concrete type because
body semantics affect planning, hooks, replay, pagination, and information
flow.

## Specification type and caller input

The declared type implements `RequestBodySpec`. Its associated input type is
what callers provide. This lets marker/specification types describe JSON,
custom codecs, streams, factories, or multipart behavior without requiring the
macro to know concrete type names.

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#buffered_requests -->
```rust
    let _: body_api::JsonRoundTripRequest = client.json_round_trip(vec![1, 2, 3]);
    let _: body_api::TextRoundTripRequest = client.text_round_trip("bounded text".to_owned());
    let _: body_api::CustomRoundTripRequest = client.custom_round_trip("custom payload".to_owned());
```

## Preparation lifecycle

One operation separates:

1. bounded structural metadata inspection;
2. before-send hook execution;
3. body preparation after the hook approves;
4. validation of the managed body against metadata;
5. per-attempt native materialization or reconstruction.

This avoids expensive or side-effecting body work when a hook rejects the
request and gives retry/recovery an explicit replay model.

## Retry and authentication recovery

A retry or authentication recovery may need another native body. Buffered
bodies and repeatable factories can be reconstructed. One-shot streams cannot
silently become empty resends. The managed body reports the capabilities needed
by the executor.

## Pagination

A body-bearing paginated endpoint must use a specification implementing
`PageRequestBodySpec`. The marker requires cloneable input and promises that
each clone independently prepares the same page-stable body while declared
pagination bindings advance.

The body itself is not a binding destination and cannot hide page-dependent
mutation.

## Streaming request bodies

`Stream` and `Stream<M>` support managed streaming input. Request-byte and
non-progress limits remain enforced. Streaming input is not automatically
replayable; the selected body implementation determines whether another
attempt is possible.

## Multipart

The `multipart` feature exposes request-only `Multipart`. Fields and streams
remain bounded by request-body limits. File content is materialized only when
body preparation begins. Multipart response parsing is not implied.

## Security and observation

Body values and bytes do not enter endpoint descriptions, safe plans, hooks,
rate keys, errors, or diagnostics. Before-send hooks receive only structural
body facts such as kind and known length.

Custom body implementations receive application data and are trusted with
safe messages, finite progress, logging, and internal state. They do not
receive the native application request.

## Why core keeps native body ownership

A native-request callback would allow arbitrary framing or signing, but would
also bypass or obscure planning effects, content-type ownership, limits,
replayability, credential boundaries, hook ordering, and one-executor
semantics. Concord chooses semantic body extension over unrestricted transport
mutation.

## Related

- [Responses](responses.md)
- [Retry](retry.md)
- [Pagination](pagination.md)
- [Extensions](extensions.md)
