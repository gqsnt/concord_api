# Integration record

## Between waves 3-4 (shared multipart format)

Library refinement R2: `concord::multipart` (feature `multipart` of `concord-extras` and the
facade), on top of Concord `7354d5a3`. Role: implementer, a Claude agent session spawned by the
coordinator (model and effort not re-derived from harness metadata here). Not an independent
review: nobody has reviewed this change yet.

**Verdict: adopted.** `upload_multipart` no longer writes MIME framing or scans for a boundary;
it builds a `multipart/related` `MultipartBody` (JSON metadata part, then the media part) and the
declared endpoint encodes it with `request Multipart;`. The public `DriveClient::upload_multipart`
keeps its signature, its 5 MiB limit, its media-type refusals (`ErrorKind::InvalidInput`, before
any HTTP), the metadata part's `Content-Type: application/json; charset=UTF-8`, the part order,
the CRLF framing and the `Content-Length` (no chunking). This answers FRICTION.md items 4 and 8
(handwritten framing; docs pointing at the native route): the docs now recommend the format.

### Declaration and method

Before (`src/api.rs` and `src/upload.rs`, `7354d5a3`):

```rust,ignore
            POST multipart(body payload: EncodedBody, query fields: String) -> File {
                query { "uploadType" = "multipart"; }
                request Encoded;
            }

        let mut sequence = 0_u64;
        let boundary = loop {
            let candidate = format!("concord_drive_{sequence}");
            if !contains(&json, candidate.as_bytes()) && !contains(bytes, candidate.as_bytes()) {
                break candidate;
            }
            sequence += 1;
        };
        let mut body =
            format!("--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n")
                .into_bytes();
        body.extend_from_slice(&json);
        body.extend_from_slice(
            format!(
                "\r\n--{boundary}\r\nContent-Type: {}\r\n\r\n",
                media.to_str().expect("validated MIME is ASCII")
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let content_type = HeaderValue::try_from(format!("multipart/related; boundary={boundary}"))
            .map_err(|_| Error::invalid("invalid multipart boundary"))?;
        let payload = EncodedBody::with_content_type(body.into(), content_type);
```

After:

```rust,ignore
            POST multipart(body payload: MultipartBody, query fields: String) -> File {
                query { "uploadType" = "multipart"; }
                request Multipart;
            }

        let payload = MultipartBody::related()
            .part(Part::new(json).header("Content-Type", "application/json; charset=UTF-8"))
            .part(Part::new(bytes.to_vec()).header(
                "Content-Type",
                media.to_str().expect("validated MIME is ASCII"),
            ));
```

The `contains` helper is gone. `Cargo.toml` adds the facade feature `multipart`. The resumable
`transfer` endpoint keeps `EncodedBody` with `request Encoded;` (one media type, no framing).

### Behaviour change

Only the boundary: the SDK's candidates `concord_drive_0`, `concord_drive_1`, ... become the
format's `concord-boundary-0`, `concord-boundary-1`, ...; the rule is the same (the first
candidate found in neither part). The bytes between the delimiters are unchanged.

### Fixtures and tests

Three tests assert an exact boundary or exact request bytes, and their inputs were authored to
contain the SDK's candidates so that a collision is exercised. With the format's candidates the
old look-alikes collide with nothing, so keeping only the expected values in step would have
turned the collision tests into plain encodings. The inputs therefore carry the format's
candidates in the same places, and the expected requests were rewritten to match:

| Test | Input change | Expected change |
| --- | --- | --- |
| `tests/protocol.rs::multipart_related_is_byte_exact_and_collision_free` | `fixtures/small.bin`: `--concord_drive_0` becomes `--concord-boundary-0` (52 to 55 bytes) | `boundary=concord_drive_1` becomes `concord-boundary-1`; `fixtures/multipart-related.bin` rewritten |
| `tests/protocol.rs::multipart_scans_metadata_too_and_invalid_media_never_sends` | metadata name `concord_drive_0.bin` becomes `concord-boundary-0.bin` | `boundary=concord_drive_1` becomes `concord-boundary-1` |
| `tests/maintenance.rs::multipart_skips_multiple_collisions_without_normalizing_name_or_media` | `NAME` and `fixtures/maintenance-new-file.json`: `concord_drive_0` becomes `concord-boundary-0`; `fixtures/maintenance-media.bin`: candidates 1 and 2 likewise | `boundary=concord_drive_3` becomes `concord-boundary-3`; `fixtures/maintenance-multipart.bin` rewritten |

`fixtures/file-created.json` states `small.bin`'s size, so `"size":"52"` became `"55"`
(`tests/workflows.rs` compares it with `SMALL.len()`). The expected `.bin` requests were rewritten
by byte substitution of the candidates (not from SDK output) and checked against an independent
reassembly of the input fixtures (delimiter, the JSON part's header, the metadata, the media
part's header, the media, the close delimiter). PROVENANCE.md records the new contents. Every
assertion is unchanged in kind: exact `content-type`, exact `Content-Length`, no chunking, exact
body. `fixtures/first-use-media.bin` (its test reads the boundary from the header) and the
README example's look-alike bytes needed no change for the tests; the README example now shows
`--concord-boundary-0`.

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-google-drive`: 5 first_use +
3 maintenance + 25 protocol + 4 workflows passed, 0 failed; 3 doctests passed (the same counts
as before the change). The repository gate (`scripts/verify.sh`) result is in the coordinator's
records.

Offline only; no Google API calls.

## Between waves 3-4 (ResponseMeta)

Concord gained `concord::ResponseMeta` (an owned head: status, version, header map, operation;
`Received::meta()`; `Debug` prints header names only; AUTHOR_GUIDE §5.11). Base `92775757`.
Role: implementer, a Claude agent session spawned by the coordinator (model and effort not
re-derived from harness metadata here). Not an independent review.

**Verdict: adopted (re-export).** `src/error.rs` now has `pub use concord::ResponseMeta;` in
place of its own struct, `capture` and `Debug`; the four capture sites (`client.rs`,
`upload.rs` ×2, `Error::provider_response`) call `received.meta()`. Decisive: the SDK type was
Concord's minus `version`, with the same names-only `Debug`, so it was a copy; the upload
session code reads `Location` and `Range` through `meta.headers()` exactly as before.

### Lines (non-blank, non-comment, from `git diff`; this file excluded)

| | removed | added |
|---|---|---|
| `src/` | 41 | 17 |
| `tests/` | 21 | 22 |
| `README.md` | 0 | 0 |

`src/` additions are the re-export, two import lines, the four `received.meta()` captures and
ten field → method edits in the SDK's own reads (`meta.status()`, `meta.headers()`,
`meta.operation()`).

### Caller-visible changes

- Fields become methods, on `Response::meta`, `ChangesPageMeta::response`,
  `UploadStatus::Incomplete { meta }`, `Error::meta()` and the `FilePages` page metadata:

  ```rust,ignore
  let location = initiated.meta.headers["location"]   // before
  let location = initiated.meta.headers()["location"] // after
  ```

  Likewise `status()` and `operation()`. New: `version()`.
- The type is now `concord::ResponseMeta` (the same type as in the other adopting SDKs). It
  cannot be built by struct literal; `ResponseMeta::new(head, operation)` builds one. No test or
  README built one.
- `Debug` (from the code): adds `version`; the operation prints by name
  (`Some("DriveApi.files.get")`, was `Some(Operation(GET DriveApi.files.get))`); header names
  only, as before. `Display` is unchanged.

### Test edits, by kind

1. Field → method, mechanical (21 lines in `tests/first_use.rs`, `tests/maintenance.rs`,
   `tests/protocol.rs`, `tests/workflows.rs`): `.status` → `.status()`, `.headers` →
   `.headers()`. Expected values unchanged.
2. Added, one assertion
   (`tests/protocol.rs::resumable_initiation_chunks_probe_and_completion_preserve_wire`): `Debug`
   of the initiation's `meta` does not contain its `Location` value (the session URL).

### Verification (observed)

`CARGO_BUILD_JOBS=6 cargo test --locked -p concord-sdk-google-drive`, run twice: `first_use` 5,
`maintenance` 3, `protocol` 25, `workflows` 4, doctests 3 passed, 0 failed (the same counts as at
`92775757`). `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` and
`cargo doc --no-deps` (no warnings): clean. Offline only.
