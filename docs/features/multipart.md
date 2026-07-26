# Multipart uploads

Use the `multipart` feature to send a bounded recipe of named text, byte, and
streaming file parts. Concord validates the recipe early and defers native
multipart serialization and boundary generation to managed execution.

## Declare and build a multipart body

The endpoint uses `Multipart` as its request-body specification:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:multipart_endpoint}}
```

Build `MultipartBody` with fallible named parts, then pass the complete recipe
to the generated method:

## Complete example

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:multipart_recipe}}
```

`MultipartPart::text` copies a bounded UTF-8 value.
`MultipartPart::bytes` retains already-owned `Bytes` without copying the
payload. Both forms are reusable, so Concord can create a fresh native body
for a permitted replay.

Attach safe concrete metadata when the service requires it:

```rust
let part = MultipartPart::bytes(bytes)?
    .file_name("report.json")?
    .content_type("application/json")?;
```

Part names, filenames, and content types are validated and counted under
finite metadata limits. Their values are omitted from Concord's debug output
and errors, but applications still own any logging they perform.

## Upload a file as a stream

Open the file through `StreamBody::from_file`, wrap it as a streaming part,
and attach its remote filename and media type:

```rust,ignore
{{#include ../../crates/concord-examples/src/request_response/bodies.rs:multipart_file}}
```

The file is opened and its current length is captured before the request is
constructed. Bytes are read during execution rather than copied into one
application buffer.

A streaming part makes the complete recipe one-shot. If authentication
recovery or an explicitly enabled retry requires a second body and the recipe
cannot be recreated, Concord reports the body failure instead of silently
reopening the path. For retryable uploads, use reusable text/bytes parts or
provide a bounded `MultipartFactory` that creates a fresh complete recipe for
each attempt.

## What Concord enforces

Recipe construction checks the number of parts, each metadata value, aggregate
metadata bytes, and inline text size. During execution Concord:

1. prepares structural body metadata without serializing private part values;
2. waits for the before-send hook and limiter to approve the attempt;
3. creates the native multipart form and random boundary;
4. streams every part through the final managed request-body boundary;
5. enforces the active aggregate request-body limit across multipart framing
   and payload bytes.

Known lower bounds can fail before transport. Unknown streaming bytes remain
cumulatively bounded. Empty chunks from a streaming part are suppressed and
counted under the request-wide non-progress ceiling rather than becoming
transport data.

Native multipart objects and boundary values never cross the public body
contract. This keeps replay decisions, finite accounting, and authentication
ordering identical to other request bodies.

## Production considerations

Treat filenames, media types, and part names as remote-contract data, not
trusted filesystem paths. Do not derive a local read path directly from
untrusted remote input. Open files with the permissions and lifecycle your
application intends, and decide how cancellation should clean up any
application-side state.

Avoid retaining a large collection of byte parts merely because each part is
individually valid. The request-body ceiling bounds transport bytes, while
your application owns memory used to construct the recipe. Use streaming for
large files and set a suitable consumer/application deadline around the
overall upload operation.

`MultipartFactory` is trusted synchronous application work. Concord checks the
operation deadline after it returns but cannot preempt or automatically bound
custom CPU, memory, I/O, queues, or retries inside the callback.

## Related

- [Request bodies and buffered responses](bodies-and-buffered-responses.md)
- [Streaming responses](streaming.md)
- [Limits reference](../reference/limits.md)
- [Finite behavior](../concepts/finite-behavior.md)
- [Extension model](../concepts/extension-model.md)
