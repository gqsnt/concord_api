# Request bodies

Body declarations separate the caller's domain input from the specification
that interprets it.

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#body_contract -->
```rust
api! {
    client BodyApi {
        timeout off;
        base "http://body.example.test/";


    POST json_round_trip(body body: Vec<u32> using Json<Vec<u32>>) -> Vec<u32> using Json<Vec<u32>>;
    POST text_round_trip(body body: String using Text) -> String using Text;
    POST custom_round_trip(
        body body: String using crate::request_response::specifications::ExampleCodec
    ) -> String using crate::request_response::specifications::ExampleCodec;

    POST upload(body body: concord::body::StreamBody using Stream) -> Bytes using Bytes;
    GET download() -> concord::response::ByteStream using Stream;
    POST exact_upload(body body: concord::body::StreamBody using Stream<OctetStream>) -> Bytes using Bytes;
    GET exact_download() -> concord::response::ByteStream using Stream<OctetStream>;

    POST multipart_upload(body body: concord::body::MultipartBody using Multipart) -> () using NoContent;
}}
```

The first body type is exactly the method argument. The `using` type implements
`RequestBodySpec<Input = DeclaredInput>`; the macro emits an endpoint-local
compile-time equality assertion. A body may occur anywhere among parameters and
keeps that source position in the generated constructor.

There is at most one body and it is endpoint-only. It cannot use `?`, defaults,
redaction, public shaping references, or post-construction controls. Optional
domain semantics use an explicit input such as `Option<UpdateInput>` and a spec
that defines how `None` encodes; Concord never infers absence from type spelling.

## Lifecycle and authority

The managed path is caller input, static metadata, core-controlled preparation,
prepared body, per-attempt/page materialization, then native body. The
specification owns input interpretation, media metadata, preparation, and an
explicit replay/page-stability contract. Core owns timing, bounds, ordering,
retry gates, page execution, and native request construction.

Mechanical replayability and semantic endpoint repeatability are independent.
Cloning does not prove pagination page stability. One-shot streams block resend.

Body values and bytes never enter contract descriptions, plans, rate keys,
ordinary gates, completion, errors, or debug. Planning may report only static
kind/media and purely known length metadata that does not inspect the input.

Request specs exclusively own `Content-Type`; public/auth header placement
cannot write it. All buffered, streaming, multipart, and factory forms remain
under configured request-body and structural bounds.

See [responses](responses.md), [retry](retry.md), and the normative
[body grammar](spec.md#request-bodies-and-responses).
