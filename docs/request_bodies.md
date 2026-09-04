# Request bodies

A request body is a dedicated semantic role. It is not a public atom, header map, arbitrary async stream, or native transport object.

## Declaration

These endpoint fragments assume their domain types and any inherited request or
response representations are declared by the surrounding compiled contract.

With an inherited request representation:

```concord-pseudocode
POST create(body input: CreateItem) -> Item;
```

With a local exception:

```concord-pseudocode
POST login(body input: LoginInput via Form) -> Token;
```

The body type is exactly what the caller passes. At most one body slot exists. Body defaults are forbidden.

## Optional body

This partial endpoint fragment assumes an inherited request representation:

```concord-pseudocode
PATCH patch_item(
    id: ItemId,
    body patch?: Option<ItemPatch>,
) -> Item;
```

States:

```text
outer None             no native body; no Content-Type; representation not invoked
Some(None)             body present; representation receives Option::None
Some(Some(patch))      body present; representation receives the patch
```

Generated controls:

```text
.set_patch(Option<Option<ItemPatch>>)
.omit_patch()
```

Do not flatten the outer Concord presence and inner Rust payload.

## Representation role

Conceptually:

```text
pub trait RequestRepresentation<Input>: Send + Sync + 'static {
    type Prepared: PreparedRequestBody;

    const DESCRIPTION: RequestRepresentationDescription;

    fn metadata(
        input: &Input,
        context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, RequestRepresentationError>;

    fn prepare(
        input: Input,
        context: RequestBodyContext<'_>,
    ) -> Result<Self::Prepared, RequestRepresentationError>;
}
```

This trait shape and authority division are selected for v2.1:

- representation sees the exact body input and bounded body context;
- it owns encoding, request media, structural metadata, and mechanical capabilities;
- core owns timing, invocation cardinality, private ordering, limits, cancellation, attempt construction, and native body adaptation;
- representation never sees method/path/query/auth/limiter/native request unless a narrow body-specific fact is explicitly part of its context.

## Lifecycle

Baseline direct/page lifecycle:

```text
exact caller input
→ structural metadata without encoding/opening/polling
→ operation begins and public head is shaped
→ prepare once for this direct/page request
→ retain managed prepared representation
→ create/reconstruct one attempt body for each admitted visible attempt
→ core adapts to native transport after private admission
```

Preparation occurs before credential readiness and rate admission so deterministic local encoding/validation failure does not consume provider quota. Attempt-only or one-shot resources should be opened as late as the prepared-body contract allows.

A producer can still fail while transport is consuming it; that is a real attempted request and consumes admission.

## Mechanical capabilities

Keep separate:

```text
ReplayableWithinRequest
  prepared body can produce another equivalent visible-attempt body

PageStableInput
  preparing a page after declared page-value changes remains semantically valid

AggregateReusablePreparedBody
  one prepared body may live/replay across the complete pagination aggregate
```

The third capability is deferred. Do not infer it from page stability. The v2.1 baseline prepares once per page and reuses only within that page’s visible attempts.

## Complete and streaming bodies

First-party caller carriers may include:

<!-- concord-source: crates/concord-examples/src/request_response/bodies.rs#streaming_requests -->
```rust
    let _: body_api::UploadRequest = client.upload(RequestStream::from_bytes(Bytes::from_static(
        b"stream payload",
    )));
    let _: body_api::DownloadRequest = client.download();
    let _: body_api::ExactUploadRequest = client.exact_upload(RequestStream::from_bytes(
        Bytes::from_static(b"binary payload"),
    ));
    let _: body_api::ExactDownloadRequest = client.exact_download();
```

A request stream is one-shot unless an explicit factory/representation proves independent reconstruction. `Clone` never implies replayability.

Multipart independently bounds:

- part count;
- names and headers;
- known and streamed bytes;
- aggregate bytes;
- empty/non-progress behavior;
- per-attempt reconstruction.

## Media ownership

The effective request representation owns default `Content-Type` and any body-specific `Content-Encoding`. Generic public headers cannot set them.

Dynamic media selection requires a representation that models media and body encoding together. It is not a generic header escape.

## Information boundary

Plans, descriptions, observers, rate keys, credential providers, and ordinary errors may expose body category/spec identity and bounded size metadata only. They never receive the input or body bytes.

## Errors

Body failures distinguish stages such as:

```text
Metadata
Prepare
AttemptConstruction
TransportConsumption
ReplayUnavailable
LimitExceeded
```

Public detail is structured, bounded, and content-free. Third-party error strings are protected because they may quote serialized values.

## Evidence requirements

- metadata does not materialize or poll content;
- prepare exactly once per direct/page request;
- retries/recovery reconstruct through prepared state;
- one-shot bodies block unsafe replay;
- local body failure occurs before provider/rate/transport;
- body limits and progress rules hold;
- optional absence invokes no representation/media behavior;
- sensitive/body sentinel values never enter public evidence.
