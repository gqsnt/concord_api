# Provenance and scope pin

Access date for every official source below: **2026-11-06 (UTC)**. Protocol version:
**Google Drive REST v3**, as served on that date. The required three sources were
re-opened during this implementation; dates below are the pages' displayed
“Last updated” dates, not a claimed immutable Google release or archived snapshot.

| Key | Official page and sections used | Displayed update |
| --- | --- | --- |
| U | [Upload file data](https://developers.google.com/workspace/drive/api/guides/manage-uploads): “Perform a multipart upload / HTTP”, “Send the initial request / HTTP”, “HTTP - multiple requests”, “Resume an interrupted upload / HTTP” | 2026-09-03 UTC |
| L | [files.list](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/list): HTTP request, query parameters, response body, nextPageToken and incompleteSearch | 2026-07-07 UTC |
| C | [Retrieve changes](https://developers.google.com/workspace/drive/api/guides/manage-changes): “Get start page token” and “Retrieve changes”, especially the closing next-page/new-start guidance | 2026-09-03 UTC |
| CL | [changes.list](https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/list): query parameters and response body | 2026-07-07 UTC |
| CS | [changes.getStartPageToken](https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/getStartPageToken): response body | 2025-04-18 UTC |
| G | [files.get](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/get): HTTP request and response body | 2026-03-20 UTC |
| N | [files.create](https://developers.google.com/workspace/drive/api/reference/rest/v3/files/create): upload URI, request/response body | 2026-03-20 UTC |
| E | [Resolve errors](https://developers.google.com/workspace/drive/api/guides/handle-errors): JSON representation, rateLimitExceeded, 404 and 5xx shapes | 2026-09-03 UTC |
| F | [files resource](https://developers.google.com/workspace/drive/api/reference/rest/v3/files): JSON representation and field definitions | 2026-07-14 UTC |
| CR | [changes resource](https://developers.google.com/workspace/drive/api/reference/rest/v3/changes): fileId, removed and file definitions | 2025-04-18 UTC |
| P | [Return specific fields](https://developers.google.com/workspace/drive/api/guides/fields-parameter): nested selections and response filtering | 2026-09-03 UTC |

## Fixture ledger

All files below are **hand-authored synthetic fixtures informed by the documented
wire structures**. None is a captured response, downloaded vendor fixture, or
verbatim vendor example. All identifiers, names, tokens, timestamps and bytes are
invented. JSON response fixtures use UTF-8, compact separators and one terminal LF;
`new-file.json` is compact UTF-8 without a terminal newline. Binary fixtures are
byte-preserved by `.gitattributes` (`fixtures/** -text`).

| File under `fixtures/` | Sources/version/access | Transformations and purpose |
| --- | --- | --- |
| `new-file.json` | U multipart metadata; N writable metadata; Drive v3, 2026-11-06 | Minimal `name` object using invented `café-東京-🛰.bin`; UTF-8 serialization, no LF. |
| `small.bin` | U multipart media; Drive v3, 2026-11-06 | Entirely invented binary payload: NUL, FF, 80, ASCII, CRLF and `--concord-boundary-0` (`--concord_drive_0`, the SDK's own first candidate, until the shared multipart format between waves 3-4; see `INTEGRATION.md`); no provider byte example was used. 55 bytes. |
| `multipart-related.bin` | U multipart/related HTTP framing; Drive v3, 2026-11-06 | Authored expected request: two MIME parts, exact `new-file.json`, exact `small.bin`, CRLF separators, closing delimiter. Boundary `concord-boundary-1` avoids the seed's collision with candidate 0 (was `concord_drive_1`; rewritten by byte substitution between waves 3-4 and checked against an independent reassembly of the two input fixtures). |
| `file-created.json` | U final metadata; G/N/F File shape; Drive v3, 2026-11-06 | Invented `file-new`, Unicode name, octet-stream and string size equal to `small.bin` length. Omit unselected fields. |
| `file-resumable.json` | U completed transfer; G/F File shape; Drive v3, 2026-11-06 | Invented `file-resumed`; string size 524325 matches the expanded workflow source. |
| `files-page-1.json` | L response body; Drive v3, 2026-11-06 | Invented first file, `incompleteSearch:false`, opaque Unicode/reserved-character token `files/+2=é`. |
| `files-page-2.json` | L response body; Drive v3, 2026-11-06 | Invented second file; no next token; several optional metadata fields intentionally omitted. |
| `start-token.json` | C get start token; CS response body; Drive v3, 2026-11-06 | Invented nonnumeric opaque `change/start+0=` string, no normalization. |
| `changes-page-1.json` | C/CL continuation body; CR change shape; Drive v3, 2026-11-06 | Invented addition record with nested selected File and next token. |
| `changes-page-2.json` | C/CL continuation body; CR removed shape; Drive v3, 2026-11-06 | Invented removed record without File metadata; an additional next token creates the required perturbation. |
| `changes-page-3.json` | C/CL terminal body; CR change shape; Drive v3, 2026-11-06 | Invented resumed-upload record and new-start token `change/poll+3=`; next token omitted. |
| `rate-limit.json` | E 403 rateLimitExceeded JSON shape; Drive v3, 2026-11-06 | Invented explanatory text; documented reason/domain names. One test deliberately pairs this body with HTTP 503 to prove that response status and body code are retained separately. |
| `expired-session.json` | U session status 404; E 404 JSON shape; Drive v3, 2026-11-06 | Invented expired-session message using documented notFound reason shape. |
| `invalid-fields.json` | E JSON error structure; P invalid-field behavior; Drive v3, 2026-11-06 | Invented 400 parameter error with `location:fields`; not represented as an exact vendor validation response. |

## Inline and generated cases

The resumable source is generated in `tests/support/mod.rs::source`: repeat the
exact `small.bin` seed and truncate to `2 * 262144 + 37 = 524325` bytes. It is a
synthetic deterministic expansion, not a downloaded binary or a provider example.
The workflow server compares each received chunk against its expected source
slice, records the accepted prefix, loses one response after accepting a chunk,
and answers probes from that state. Exact reconstruction is asserted independently
of any submitted byte-count assumption.

All Location, Range, Content-Range, authorization and diagnostic header values in
tests are generated synthetic values. The loopback host/port comes from the owned
server. MIME-invalid, repeated/missing/cross-origin Location, range overflow,
range regression, premature completion, missing/ambiguous/cyclic page boundary,
incomplete search, HTML errors, truncated/oversized JSON, and cancellation cases
are adversarial SDK tests, not claims that Google returns those cases. The final
empty changes poll is inline synthetic JSON that advances the saved boundary.
Body hangups, delayed heads and truncation are `examples/testserver` behaviors.

The upload guide's prose uses human ordinal byte numbering near its `0-42` range
example. This SDK implements the wire arithmetic: inclusive range end 42 means
43 acknowledged bytes and next zero-based offset 43. The bounded positive/negative
range tests use that arithmetic. We follow the documented boundary semantics,
not the guide's language-client snippets as a pagination algorithm.

## Concord inputs and dependency choices

Read from frozen `6d6d66aa`: root README; author/caller/format/recipe/diagnostic
guides; crate public rustdoc generated with `cargo doc -p concord --no-deps
--features csv,jsonl`; the core, extras, macros and pagination READMEs; GitHub SDK
API/context/error/paging files; Stripe's bounded error sender; and the shared
loopback server. GitHub/Stripe supplied response-adapter patterns. Runtime-body
examples supplied the EncodedBody binding pattern. No provider SDK HTTP transport
was copied or delegated to a Google SDK. No private Concord API is used.

`serde`/`serde_json` handle JSON, `mime` validates concrete media types, and
`concord-pagination` handles bounded lazy traversal and partial outcomes. The
facade exposes reqwest, bytes, Json and Encoded. `mime` was chosen instead of
reimplementing MIME grammar. The related serializer is a fixed two-part SDK
serializer because reqwest multipart produces form-data. No signing/XML protocol
is in this OAuth bearer + JSON slice. Dev dependencies are the path testserver,
Tokio for offline async tests, and futures-util to verify the public item stream.
All dependency versions already existed in the workspace lock; the lock change
adds this SDK package's dependency list only.

No live provider requests, credentials, OAuth minting, or vendor-conformance
verification occurred. Execution evidence is offline fixture evidence only.

## Maintenance fixture additions (2026-11-06)

Re-opened the official [upload guide](https://developers.google.com/workspace/drive/api/guides/manage-uploads)
(HTTP multipart, multiple requests, and interrupted-upload sections) and
[changes.list reference](https://developers.google.com/workspace/drive/api/reference/rest/v3/changes/list)
(response boundary fields). These describe the protocol structure; none of the
new bytes or documents was downloaded or copied from an example. All remain
hand-authored synthetic inputs and expected output, covered by `fixtures/** -text`.

| Fixture | Authored bytes and purpose |
| --- | --- |
| `maintenance-new-file.json` | 52 bytes of compact UTF-8 JSON, no terminal LF. The name contains `e` + U+0301, Japanese text, a satellite emoji, and `concord-boundary-0` (was `concord_drive_0`, 49 bytes, before the shared multipart format). No Unicode normalization is applied. |
| `maintenance-media.bin` | 66 invented bytes: NUL, FF, 80, FE, CRLF, a candidate-1 delimiter line and a candidate-2 closing-delimiter line (`concord-boundary-1`/`-2`; `concord_drive_1`/`_2`, 60 bytes, before the shared multipart format). Ends in CRLF. |
| `maintenance-multipart.bin` | 281 hand-assembled expected bytes: candidate 3 framing, the exact metadata and media fixtures, CRLF separators and final delimiter. Authored independently of SDK output (263 bytes with `concord_drive_3` before the shared multipart format; rewritten by byte substitution and checked against an independent reassembly). |

`tests/maintenance.rs` repeats the 66-byte seed and truncates to
`2 * CHUNK_ALIGNMENT + 37 = 524325` bytes for both restart cases. The server
commits the first chunk; the partial-acceptance case commits only 17 bytes of
the next request and closes the connection without a response. This models
uncertain provider progress using the local server's hangup facility; it is not
a TCP-level truncated request capture. Expected ranges, final metadata size,
and exact reconstruction derive from the immutable source.

Inline synthetic JSON uses `serde_json::json!` for metadata and changes pages.
Invented opaque token constants contain spaces, plus signs, literal percent
sequences, Unicode and equals signs. The server returns a synthetic session URL
with unusual query escape spelling, repeated pairs and an empty value. These
probe preservation, not the actual format of Google's tokens or upload IDs.
The existing `rate-limit.json` supplies the intervening 403 response. No original
fixture or first-use test was changed.
