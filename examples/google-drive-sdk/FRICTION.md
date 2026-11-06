# SDK12 author friction — Google Drive v3

Baseline: frozen Concord `6d6d66aa`; no library, script, check, documentation or
other-example edits. Facts below describe authored declarations, adapters, caller
paths and observed diagnostics. Reproduction paths are relative to this SDK unless
explicitly repository-relative. These are offline fixture results, not live Drive
verification. No private reach-through, reconstructed HTTP execution, native
request mutation, or handwritten operation-identity string was needed.

To run an individual reproduction from the workspace root, prefix every command:

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-google-drive --test protocol TEST_NAME
```

## 1. Runtime body/media binding worked through an ordinary declared body

**Cause: Concord, positive.** `src/api.rs:28` declares `EncodedBody` with
`request Encoded`; `src/upload.rs:110` binds the selected related boundary to the
body. `multipart_related_is_byte_exact_and_collision_free` at
`tests/protocol.rs:69` checks the complete request against a byte fixture and
observes a matching Content-Type and nonempty Content-Length. There is no native
multipart builder, custom Encode implementation, duplicated decoder, or dynamic
Content-Type setter. The public facade supplied everything needed at this seam.

## 2. Response heads, bounded body reads and operation identity survived failures

**Cause: Concord, positive; provider schema remains SDK work.**
`src/client.rs:126` calls `Prepared::receive` exactly once;
`src/client.rs:130` captures the head before `decode_limited`.
`src/error.rs:20` obtains the generated operation, actual status and headers.
`successful_metadata_failures_keep_response_information` at
`tests/protocol.rs:701` passed for bad media type, malformed JSON, over-limit
metadata and a truncated successful body. Each error retained status/request-id.
`malformed_oversized_or_truncated_error_bodies_keep_the_head` at
`tests/protocol.rs:668` passed for bounded non-success bodies. No execute/finish
reconstruction, decoder recreation, fabricated status or operation name was used.

## 3. Resumable 308 and uncertain progress need an SDK state machine

**Cause: provider protocol, implemented in the SDK.** `src/upload.rs:304` manually
selects 308 progress, 200/201 completion, 404 expiration and other failures before
choosing a body consumer. The ordinary declaration's JSON send would reject 308
as non-2xx; `Received` makes the necessary dispatch possible without a native
escape. `src/upload.rs:247` marks the session uncertain before awaiting its write;
`src/upload.rs:287` probes with an empty PUT. Both transfer and probe keep the
same generated operation identity (`DriveApi.uploads.transfer`).

Reproduce with `resumable_initiation_chunks_probe_and_completion_preserve_wire`
at `tests/protocol.rs:141`, `lost_final_response_is_resolved_by_probe_without_replaying_bytes`
at `tests/protocol.rs:448`, or the restart workflow at `tests/workflows.rs:162`.
Observed results: 308 remained typed progress, a lost response blocked further
writes, a status probe recovered the committed offset, and the state machine sent
only the remaining source suffix. The acknowledged offset is not inferred from
submitted length. Cancellation, invalid ranges and malformed final metadata also
leave the probe guard set. No retry is hidden in the adapter.

## 4. Multipart/related framing is authored SDK code

**Cause: provider requires related; SDK serializer work, with possible generic
multipart-library reuse outside this slice.** `src/upload.rs:86` serializes JSON,
scans both parts for boundary collisions, and writes MIME framing/CRLF manually.
The boundary-to-body binding is provided by Concord, but MIME composition is not.
Reqwest's multipart helper produces form-data. `mime` was selected for mature
media-type grammar validation; no general MIME parser was handwritten.

Reproduce with `multipart_related_is_byte_exact_and_collision_free` at
`tests/protocol.rs:69` and `multipart_scans_metadata_too_and_invalid_media_never_sends`
at `tests/protocol.rs:97`. Both passed. The binary contains candidate boundary 0;
metadata can also contain it; candidate 1 is selected and exact bytes survive.
This is the largest generic serialization block authored outside the declaration.

## 5. Public domain operations still wrap declaration preparation and error adaptation

**Cause: SDK API design and provider error schema; residual adapter boilerplate.**
`src/client.rs:145`, `src/client.rs:203`, `src/client.rs:216` and
`src/client.rs:232` project domain arguments into generated operations and call the
shared receive/finish adapter. `src/error.rs:181` manually decodes Drive's error
envelope, distinguishes malformed/large/failed bodies, and retains metadata.
`src/upload.rs:123` and `src/upload.rs:304` add provider-specific status handling.
Concord has a public SendWith policy facility; this SDK uses Prepared/Received
inside its domain client so uploads can enforce their state invariants.

Reproduce with `errors_keep_typed_documents_headers_and_safe_diagnostics` at
`tests/protocol.rs:629` and the workflow A caller at `tests/workflows.rs:122`.
The caller receives typed errors and metadata directly, with no extension trait
import or manual response decode. Initial clippy reported this SDK's error was at
least 168 bytes (`result_large_err`); boxing its optional ProviderError at
`src/error.rs:126` resolved it without changing the caller's Result signatures.
That error layout issue was SDK-authored, not a Concord failure.

## 6. A changes batch has a terminal token beyond ordinary item exhaustion

**Cause: provider boundary semantics plus application commit policy.**
`src/client.rs:263` returns page metadata with `newStartPageToken` at
`src/client.rs:282`. `src/model.rs:90` and the list adapters prevent partial-field
selection from dropping the control fields. Concord pagination transports that
metadata, but `.items()` discards it; the SDK documentation directs changes
consumers to pages. No separate hidden token store or fabricated cursor is used.

Reproduce at `tests/workflows.rs:162`: two pages reach a required cap, the retained
next-page token resumes the additional page, the terminal token survives, and the
next polling request sends it exactly. `changes_queries_and_token_boundaries_are_exact`
at `tests/protocol.rs:740` asserts reserved-character encoding and boundary fields.
The page/item APIs already provide typed bounds and failures; only the Drive
boundary projection and persistence guidance were authored here.

## 7. Content-Length is transport-owned; an empty PUT omits it

**Cause: initial SDK mistake, plus a documentation precision issue for zero bytes.**
The first transfer declaration attempted `header(CONTENT_LENGTH) length: u64`.
It compiled with the native header constant but failed when preparing a request:

```text
PUT DriveApi.uploads.transfer: request not built: `CONTENT_LENGTH` on
`DriveApi.uploads.transfer` names a header field that reqwest/hyper manage;
it cannot be declared
```

The first full protocol execution had 13 passing/9 failing tests from this one
Build failure. Removing that declaration and relying on the owned byte body's
length fixed nonempty chunks. The next run had 21 passing/1 failing test: the test
expected `Content-Length: 0`, but pinned reqwest omitted it on an empty PUT. The
HTTP/1.1 request has no Transfer-Encoding and no body, so it is empty without an
explicit length. No native header mutation was added. The final wire assertion
is in `resumable_initiation_chunks_probe_and_completion_preserve_wire` at
`tests/protocol.rs:141`; the probe body is constructed at `src/upload.rs:291`.
The frozen format guide's broad statement about byte-body Content-Length
(repository `docs/CUSTOM_FORMATS.md:665`) does not mention this zero-byte case.
This is fixture-verified transport behavior, not a live Google acceptance claim.

## 8. Older documentation still directs dynamic multipart toward native plumbing

**Cause: Concord docs.** Repository `docs/CUSTOM_FORMATS.md:51` and
`docs/RECIPES.md:277` recommend native request construction for a runtime boundary,
while `docs/CUSTOM_FORMATS.md:625` and public EncodedBody rustdoc describe the
new runtime-body path. Reading the later section avoided unnecessary
`into_parts`/execute/finish work. Reproduce the working path in
`src/api.rs:28` and `src/upload.rs:112`, with the exact MIME test at
`tests/protocol.rs:69`. No frozen documentation was edited.

## 9. Small caller discovery correction; prefixes remain available

**Cause: SDK test-author lookup mistake.** The first protocol test build accessed
`Incomplete.items`; rustc E0609 reported the available fields `collected` and
`error`. Replacing the test access with `collected` corrected the build.
`page_bounds_failures_empty_pages_and_item_streams_are_explicit` at
`tests/protocol.rs:819` now verifies retained prefixes, typed cap/provider failures,
an empty nonterminal page, and the standard item Stream. The public pagination
README and rustdoc supplied the intended interface. No scheduler or traversal
engine was rebuilt in this SDK.

## Verification record

- An initial unlocked package test added only this SDK's lockfile package entry;
  no external dependency versions changed.
- Final observed package test: **25 protocol + 4 workflow + 3 compiled README
  examples passed**; no live calls. The README is included as crate rustdoc so its
  construction and both workflow code blocks are compiled by the package test.
- Package clippy with `--all-targets -- -D warnings` passed after the SDK error
  layout and needless test-borrow corrections. Package rustfmt completed.
- New byte fixtures use `fixtures/** -text`; `git check-attr` reports text unset
  for the multipart binary fixture.
- Changes are confined to this example, its workspace member, and its lock entry.
  `tests/first_use.rs` was not created; independent consumer work remains reserved.
