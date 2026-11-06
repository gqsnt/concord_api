# Independent first use — Distribution registry v2

## Context and boundaries

Consumer work began at HEAD `bdea8b710d23bb08c19c6be6db6a063d44841bae`, using the pinned
Rust 1.99.1 toolchain and the shared `target-b` cache with five Cargo jobs. The requested runner
is Codex gpt-6.1-sol (xhigh); no alternate model or delegated agent was used. The Concord
library is frozen for this task. Only this record, the new `tests/first_use.rs` and new
consumer fixtures will be written. No held-out campaign records, author friction notes, SDK source or
existing SDK tests have been read. No other provider SDK supplied the solution.

## Outcome A prediction — written before its test implementation

First entry point: SDK README “Construction”, then “Workflow A: discover, pull, verify”.
`Registry::catalog` sends nothing until `next_page`; its first GET is `/v2/_catalog?n=...`.
Subsequent requests follow the server's next Link, even if its page size changes. A page
without a next Link yields one final page, then `Ok(None)`. A failed page yields a typed
`PagesError::Provider`; collection keeps its prefix and cannot report successful exhaustion.
Tags behave similarly under `/v2/<name>/tags/list`; a null tags list is empty.

A manifest GET sends the explicit supported Accept values and returns parsed descriptors,
exact bytes, computed digest and metadata. For each descriptor, HEAD returns size information
(or `None` for 404); GET through `blob_for` verifies size and digest. A chunk remains unverified
until the reader reaches `Ok(None)`. A clean HTTP body shorter than its descriptor should fail
integrity; a broken chunked body should fail transport and retain the response metadata.
Wrong bytes of the correct length should fail digest verification. A documented registry error
should retain its HTTP status, typed code, detail and operation without printing body text.

The fake provider will implement paged catalog/tags, a schema-2 manifest and blob HEAD/GET,
with independently authored byte fixtures. It will check request methods, raw targets, Accept,
empty GET/HEAD bodies, and actual bytes. Separate cases cover failed pagination, a missing
manifest error document, null tags, damaged blob contents and truncated bodies.

## Ordered trace

1. Read the SDK README and package manifest, root README, Concord caller guide (including
   streams/pages), recipes (SDK/application patterns), and pagination crate README. Read the
   testserver helper's public interfaces in `examples/testserver/src/lib.rs` to construct my fake;
   this is not a read of the SDK source or tests.
2. Built SDK rustdoc with `cargo doc --locked -p concord-sdk-distribution-registry --no-deps`
   and Concord rustdoc with `cargo doc --locked -p concord --no-deps --features csv,jsonl`.
   Both passed. Every Cargo invocation uses the required jobs/cache environment prefix.
3. Consulted the official [HTTP API v2](https://distribution.github.io/distribution/spec/api/),
   [token authentication](https://distribution.github.io/distribution/spec/auth/token/) and its
   linked [schema-2 manifest](https://distribution.github.io/distribution/spec/manifest-v2-2/)
   documentation. No live provider or token endpoint was contacted.
4. Read generated public rustdoc for Registry, Repository, BlobReader, listing options,
   manifest/results, metadata and errors. A local HTML-to-text attempt lacked `bs4`; switched
   to Python's standard-library HTMLParser. Rustdoc re-exports live under module paths rather
   than root `struct.*.html` files. Neither required a source read.
5. Wrote outcome A from the public README/rustdoc and independently authored fixtures. The first
   compile produced one distinct diagnostic: `E0382: borrow of moved value: items`.
   `PageItems::collect_complete(self)` consumes the traversal, so a subsequent `items.next()`
   terminal-state assertion does not compile. Kept the incomplete prefix/resume assertions and
   checked `PagesError::Ended` with the separate page-by-page bad-Link case. This was a caller
   correction, not an SDK source-read intervention or a native escape.
6. The first executable outcome-A run compiled and passed 7/8 tests. The remaining assertion
   expected both Display and Debug to hide provider message text. Debug contained the synthetic
   error message. The README promises body-text redaction for **Display**, so narrowed the test
   to that documented contract. Concrete logging friction: formatting a missing-manifest
   `RegistryError` with `{:?}` exposes its provider-supplied message; safe summary logging must
   deliberately use Display. Reproduce with
   `missing_manifest_keeps_typed_provider_document_and_redacted_diagnostics` and format its error
   with `{:?}`. No source read was needed to identify the mismatch.

## Outcome B prediction — written before its test implementation

First entry point: README “Workflow B: chunked upload, resume, publish” and “Upload semantics”,
then public rustdoc for BlobUpload, UploadProgress, BlobPushed, ManifestUpload and ImageManifest.
Start is an empty POST yielding a session at offset zero from a 202 response. PATCH sends raw
bytes, `application/octet-stream` and the inclusive `Content-Range`. Each acknowledgement
moves the offset and adopts the latest Location, preserving opaque query bytes. UUID is only
correlation metadata. A lost PATCH answer leaves `needs_status()` true; another PATCH or PUT
must fail locally with StatusRequired. GET of the last known Location returns authoritative
progress (204, Range), after which the application re-slices its original bytes at that offset.
A 416 adopts the reported Range/Location while returning a typed API error.

PUT completion appends digest to the latest query, requires 201 and matching digest, returns
BlobPushed and seals the session. Calls after completion fail locally with UploadFinished.
DIGEST_INVALID is a typed provider document and requires restarting the upload, not claiming
success. The documented ambiguous zero/one-byte range means a one-byte first PATCH is refused;
a one-byte blob can be sent in the completing PUT. A convenience `push_blob` stops on its first
failure; recovery needs the explicit session handle and an application attempt limit.

The stateful fake will accept a chunk but lose its reply, report a shorter committed prefix,
then accept the application's resumed bytes without duplication. It will rotate opaque
Locations, hash stored bytes independently, retain uploaded blobs and reject a manifest whose
referenced blobs are absent. A successful manifest PUT must store exactly the declared bytes
and be readable afterward. Auth will use a separate loopback token service: a pull token must
not be sent with a push; the push challenge obtains its own scope, and retries once. Synthetic
`token` and `access_token` answers will exercise the documented aliases and cache. Further cases
will check 416 recovery, digest refusal/restart, missing manifest blobs, an untrusted realm and
a second 401 ending the exchange without an endless retry.

## Ordered trace (continued)

7. After the two caller corrections, all 8 outcome-A tests passed. The fake's paged raw targets,
   nested repository paths, manifest Accept order, exact fixture bytes, blob HEAD lengths,
   null tags, error detail, failed-page prefix/resume and three damaged-body cases matched the
   predictions. No SDK source or existing tests were read.
8. Wrote outcome B using the documented session handle and a separate token-service fake.
   The fake hashes stored bytes with the existing `sha2` dependency directly, independently of
   the SDK's Digest helper. Its first compile produced `E0277`: sha2 0.11's output array does not
   implement LowerHex. Changed the fake helper to format each output byte with `{:02x}`.
   This is third-party formatting/caller friction, not a Concord or SDK workaround.
9. The first executable B run passed 14/15 tests, including the complete stateful resume/publish
   workflow. The remaining test had again assumed Debug suppresses arbitrary provider fields:
   a final 401's Debug included the public challenge realm URL. Display did hide it and both
   formatters hid the token. Kept token-redaction checks for both formatters and URL-redaction
   checks for Display, as the README promises. This extends the logging observation in entry 6;
   it did not require reading the implementation.

10. Corrected the logging assertions and formatted only the new test file with Rustfmt.
    Final requested test run: **15 passed, 0 failed**, offline on loopback. Rustfmt check and
    whitespace checks passed. Independently rechecked the fixture descriptor sizes/digests
    using Python hashlib. Git attributes report `text: unset` for the binary layer and manifest.
    The worktree changes are only this record, `tests/first_use.rs`, and five new consumer
    fixture/provenance files. Frozen library paths match `47a25fbf`; existing SDK files are
    unchanged.

## Working paths and concrete friction

- **A:** `Registry::ping` → lazy catalog pages → collected tags → manifest by tag and digest →
  every descriptor's HEAD and verified streaming GET. A failed page retains its collected
  prefix and exact failed-page continuation; resumption requests only that page. A null tag list
  exhausts, a bad Link fails, and short/corrupt/broken blobs never produce verified EOF.
- **B:** scoped Bearer exchange → explicit BlobUpload → acknowledged chunk → uncertain chunk →
  status at offset 15 → source re-sliced at 15 → completion → convenience config push → manifest
  publication → manifest/blob readback. The fake's independent hashes and stored bytes confirm
  no duplicated or missing data. Latest Location query bytes stay intact; a 416 updates progress;
  digest rejection causes an application restart; completed sessions reject further calls.
- **Interventions:** none involving SDK source, existing SDK tests, private APIs, native transport
  escapes, rebuilt execution or invented operation identities. Only public SDK/Concord APIs and
  documented public rustdoc were needed. No frozen library defect or required workaround was
  demonstrated.
- **Consuming collection:** entry 5's E0382 is reproducible with
  `let incomplete = items.collect_complete().await; items.next().await;`. Keep the returned
  incomplete prefix/continuation, or use `next`/`next_page` for a retained traversal. This was a
  caller ownership mistake, not an SDK defect.
- **Logging:** entries 6 and 9 reproduce Debug exposing provider messages and the challenge realm.
  Display meets the documented summary contract; tokens remained redacted in both. Applications
  must choose the formatter intentionally rather than assume all provider fields are hidden.
- **Resume policy:** `resume_uncertain_chunk_then_publish_and_read_back_with_scoped_auth` shows
  that a lost reply requires retaining the explicit upload handle, querying status, re-slicing
  at the acknowledged offset and imposing an application attempt cap. The convenience method
  does not expose a failed session for recovery; this limitation is documented.
- **One-byte ambiguity:** `range_refusal_moves_the_session_and_one_byte_first_patch_is_local`
  reproduces OneByteFirstChunk without sending a PATCH. The separate one-byte whole-blob test
  confirms the documented start-plus-PUT path works. This is a documented protocol constraint.

Exact final command (the same prefix was used for both rustdoc builds and every test attempt):

```bash
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-distribution-registry --test first_use
```

These tests validate an independently authored local fake and synthetic fixture bytes. They are
not live registry or vendor-conformance evidence. Fixture origins and byte hashes are in
`fixtures/first-use-PROVENANCE.md`.
