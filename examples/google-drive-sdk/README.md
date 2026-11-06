# Google Drive v3 SDK slice

`concord-sdk-google-drive` is an unpublished, edition 2024 SDK built with
`concord::api!` against frozen Concord commit `6d6d66aa` and Rust 1.99.1.
It covers these connected operations:

| Public operation | Drive operation |
| --- | --- |
| `list_files_page`, `files` | `files.list`, opaque pagination and partial fields |
| `get_file` | `files.get` metadata |
| `create_file` | Metadata-only `files.create` |
| `upload_multipart` | `files.create`, two-part `multipart/related` |
| `initiate_upload` | `files.create?uploadType=resumable`, returned session Location |
| `UploadSession::send_chunk`, `probe`, `checkpoint` | Bounded resumable transfer, acknowledged position and restoration |
| `start_page_token`, `list_changes_page`, `changes` | `changes.getStartPageToken`, `changes.list` and both boundary tokens |

Listings and changes use the user-scoped context with `spaces=drive`. Sharing, export, media
downloads, update/delete, shared-drive configuration, all Drive metadata fields,
background sync, and OAuth token acquisition/refresh are excluded. The SDK models
nine file fields, with optional values for fields omitted by a partial response.
Query and ordering expressions use Drive's own syntax; the SDK does not parse them.

## Construction

Inject an OAuth bearer token value without the `Bearer ` prefix. The synthetic
value below is deliberately unusable. The client creates a pooled reqwest transport
with redirects and automatic retries disabled, a 30-second request timeout,
2 MiB metadata and 64 KiB error-response limits, and an 8 MiB chunk limit.

```no_run
use concord_sdk_google_drive::{ClientConfig, DriveClient};
use std::time::Duration;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let drive = DriveClient::new("SYNTHETIC_TEST_BEARER_DO_NOT_USE")?;
let configured = DriveClient::with_config(
    "SYNTHETIC_TEST_BEARER_DO_NOT_USE",
    ClientConfig {
        timeout: Duration::from_secs(15),
        max_chunk_bytes: 1024 * 1024,
        ..ClientConfig::default()
    },
)?;
# let _ = (drive, configured);
# Ok(()) }
```

For an offline server set `ClientConfig::origin` to its literal loopback origin,
for example `http://127.0.0.1:12345`. Other HTTP origins, credentials in URLs,
non-root paths, queries, and fragments are rejected. An explicit HTTPS origin is
also supported. A session URL must remain at that configured origin and exactly
`/upload/drive/v3/files`. All redirects remain visible as errors. Session query
parameters come directly from Location; the SDK never reconstructs an upload ID.
The client intentionally owns transport construction to enforce these policies;
it does not accept an arbitrary reqwest client with unknown redirect/retry settings.

## Workflow A: list, inspect, multipart-create, retrieve

Partial-field selections always include file `id`; list calls also select their
control fields, preventing a projection from accidentally hiding a continuation.
Multipart media is limited to 5 MiB and buffered with its metadata. Both MIME parts
are scanned when selecting the boundary. Unicode names and arbitrary binary bytes,
including boundary-like sequences, pass through unchanged.
Names remain JSON metadata, with no Unicode normalization or MIME filename
encoding; a decomposed combining character remains decomposed.

```no_run
use concord_sdk_google_drive::{
    DriveClient, File, FileField, FileFields, ListOptions, NewFile,
    pagination::Resume,
};
use std::num::NonZeroUsize;

async fn workflow_a(drive: &DriveClient) -> Result<File, Box<dyn std::error::Error>> {
    let fields = FileFields::only(&[FileField::Name, FileField::MimeType, FileField::Size]);
    let files = drive.files(
        ListOptions { fields: fields.clone(), query: Some("trashed = false".into()),
                      ..ListOptions::default() },
        Resume::Start,
        NonZeroUsize::new(50).unwrap(),
    ).items().collect_complete().await?;
    if let Some(first) = files.first() {
        let metadata = drive.get_file(&first.id, &fields).await?;
        println!("file id: {}", metadata.value.id);
    }
    let bytes = b"\0\xff\r\n--concord_drive_0\r\nbinary\0";
    let created = drive.upload_multipart(
        NewFile { name: "café-東京-🛰.bin".into(), ..NewFile::default() },
        "application/octet-stream", bytes, &fields,
    ).await?;
    Ok(drive.get_file(&created.value.id, &fields).await?.value)
}
```

`create_file` accepts the same metadata without media, for example with
`mime_type: Some("application/vnd.google-apps.folder".into())` to create a folder.
`NewFile` also supports description and parent identifiers.

## Workflow B: recover a session, finish, consume changes

Obtain a changes start token before the work whose changes you want to observe.
An upload session fixes the total length and MIME type. Each non-final chunk must
have a length divisible by `CHUNK_ALIGNMENT` (256 KiB); the final chunk may be
smaller. Read the source beginning at `acknowledged()`, rather than assuming a
whole submitted chunk was accepted.
The acknowledged offset can be unaligned after a partial acceptance; the alignment
rule applies to the next non-final chunk's length. Save an upload checkpoint after
an acknowledgement and before sending more bytes, and retain the changes token
alongside it. Restoring requires a probe even after stopping between requests.

The following code resumes a saved session from an interruption, finishes within
an application request budget, then returns a finite changes batch and its next
polling token. Changes are collected here so the caller can commit the batch and
token together. For larger feeds, process `next_page()` and persist its checkpoint
only after committing that page.

```no_run
use concord_sdk_google_drive::{
    CHUNK_ALIGNMENT, Change, ChangesOptions, DriveClient, File, FileFields,
    NewFile, PageToken, UploadCheckpoint, UploadStatus,
};
use std::{io, num::NonZeroUsize};

async fn begin(drive: &DriveClient, total: u64)
    -> Result<(PageToken, UploadCheckpoint), Box<dyn std::error::Error>>
{
    let start = drive.start_page_token().await?.value.start_page_token;
    let session = drive.initiate_upload(
        NewFile { name: "café-東京-🛰.bin".into(), ..NewFile::default() },
        "application/octet-stream", total, &FileFields::default(),
    ).await?.value;
    // Persist the checkpoint in protected storage before sending chunks.
    Ok((start, session.checkpoint()))
}

async fn resume_and_poll(
    drive: &DriveClient,
    checkpoint: UploadCheckpoint,
    source: &[u8],                 // same immutable source as the original session
    saved_changes_token: PageToken,
    mut persist: impl FnMut(&UploadCheckpoint) -> io::Result<()>,
) -> Result<(File, Vec<Change>, PageToken), Box<dyn std::error::Error>> {
    let mut session = drive.restore_upload(checkpoint)?;
    if session.total() != source.len() as u64 {
        return Err(io::Error::other("source length changed").into());
    }
    // Mandatory after restoration or uncertain progress; it may reveal completion.
    let mut status = session.probe().await?;
    let mut remaining_requests = 1024;
    let file = loop {
        // This application's bound is separate from the per-chunk byte bound.
        match status {
            UploadStatus::Complete(response) => break response.value,
            UploadStatus::Incomplete { .. } => {}
        }
        if remaining_requests == 0 {
            persist(&session.checkpoint())?;
            return Err(io::Error::other("upload request budget reached").into());
        }
        remaining_requests -= 1;
        // Save before the await, so cancellation also has a resumable checkpoint.
        persist(&session.checkpoint())?;
        let offset = usize::try_from(session.acknowledged())?;
        let end = source.len().min(offset.saturating_add(CHUNK_ALIGNMENT));
        status = match session.send_chunk(&source[offset..end]).await {
            Ok(status) => status,
            Err(error) => {
                persist(&session.checkpoint())?;
                // A cancelled/uncertain send leaves needs_probe() true. The next
                // invocation restores and probes; it never blindly repeats bytes.
                return Err(error.into());
            }
        };
    };

    let pages = drive.changes(saved_changes_token, ChangesOptions::default(),
                             NonZeroUsize::new(50).unwrap())
                     .collect_complete().await?;
    let next_poll = pages.last()
        .and_then(|page| page.meta.new_start_page_token.clone())
        .ok_or_else(|| io::Error::other("missing terminal change boundary"))?;
    let changes = pages.into_iter().flat_map(|page| page.items).collect();
    // Commit changes and next_poll atomically, then start the next polling interval.
    Ok((file, changes, next_poll))
}
```

`tests/workflows.rs` exercises a stronger interruption: one chunk is acknowledged,
the server commits the next chunk but drops its response, a serialized checkpoint
is restored, and an empty PUT discovers the accepted prefix. Only the remaining
37 bytes are sent. Three changes pages follow, including a budget stop/resumption
before the final `newStartPageToken`, then a further poll with that saved token.

`tests/maintenance.rs` adds three wire regressions: multiple boundary collisions
across metadata and media with a decomposed Unicode name; restart immediately
after an acknowledged chunk; and restart from the same saved checkpoint after
only 17 bytes of the next chunk are committed and its response is lost. Both
restart cases reconstruct the exact source, preserve the returned session query
(including escape spelling and repeated parameters), and retain opaque changes
tokens through an empty continuation page, a budget stop, a typed provider error,
and the extra terminal page. `MAINTENANCE.md` records why no runtime or caller
contract change was necessary.

## Completion, errors and partial outcomes

- Every HTTP request executes once. Metadata-only and multipart creates can have
  uncertain outcomes after a lost response; the application must reconcile before
  deciding to create again. No retry, token refresh, sleeps, or background work is
  hidden in an operation.
- `Response<T>` keeps the actual status, headers, and generated Concord operation.
  `Error::provider()` exposes the typed Drive code/message/reasons;
  `Error::meta()` retains the head on provider, decode, metadata and body failures.
  HTML/proxy errors, over-limit error documents, and failed body reads have a
  `DocumentProblem` classification. Before any response, metadata is absent.
  `Error::concord()` preserves the underlying error kind.
- A resumable `308` is `UploadStatus::Incomplete`, with an acknowledged byte count
  parsed from Range. A missing Range means zero; malformed, repeated, regressing,
  overflowing, or impossible acknowledgements fail closed. HTTP 200/201 plus valid
  metadata confirms completion. A 404 on transfer/probe is `SessionExpired`;
  create a new session deliberately. Other HTTP failures remain typed errors.
- A polled write sets `needs_probe` before execution. An error or cancellation
  leaves that flag set, and further `send_chunk` calls fail locally until a
  successful probe. Failed final-metadata decoding also requires probing.
  Checkpoints retain only acknowledged state and the sensitive session URL.
  Restoring always requires a probe and validates the origin again. Restoring
  under the same origin with a different account, or resuming against changed
  source bytes, is the application's responsibility. Protect checkpoints from
  disclosure/tampering and do not use concurrent copies of the same session.
- Files/changes traversals require a nonzero page budget. `Ok(None)` means provider
  exhaustion; `PagesError::Bounded` and `Provider` retain resumable positions.
  `collect_complete()` returns an `Incomplete { collected, error }` on a bound or
  failure, preserving the accepted prefix. Once an error ends a traversal, later
  reads return `Ended`, never successful exhaustion. `checkpoint()` describes the
  next page, not a transaction commit; save it after processing the returned page.
- Empty pages with continuation tokens are followed. Immediate repeated tokens are
  rejected. Longer cycles stop at the required budget. A direct files page exposes
  `incomplete_search`; a file traversal treats it as failure instead of complete
  search results. A rejected/expired files token needs a deliberate fresh listing;
  the SDK does not merge that restart with previously emitted items.
- A changes page must have exactly one boundary: `nextPageToken` for the current
  batch, or `newStartPageToken` for the next interval. The page traversal preserves
  the latter in `ChangesPageMeta`. `.items().into_stream()` is available, but
  flattening discards page metadata; consume changes as pages to retain boundaries.
- Debug/Display for clients, errors, tokens, sessions and checkpoints withhold
  credentials, session URLs and provider text. Metadata headers, error documents,
  model values, `session_url()`, and serialized checkpoints are explicitly
  accessible and unredacted. Do not log those values indiscriminately.

Finite responses are buffered within the configured byte bound. Each upload
chunk is copied once into an owned HTTP body; there is no file-reader abstraction
or unbounded lazy upload. Multipart framing adds metadata bytes beyond its 5 MiB
media cap. Empty PUT status probes have no Content-Length or Transfer-Encoding
header with pinned reqwest 0.13.5; their body is empty. Nonempty body lengths are
provided by reqwest automatically. Session expiration is provider-controlled
(documented as one week), with no local timer or automatic re-initiation.

## Dependencies, sources and validation

Production dependencies are the Concord facade (including its JSON and encoded
body formats), `concord-pagination`, `serde`/`serde_json` for Drive documents, and
`mime` for mature MIME validation rather than handwritten media-type parsing.
The small two-part related serializer is SDK code: reqwest's multipart support
produces form-data, whereas Drive requires related. There is no vendor SDK,
signer, OAuth implementation, or HTTP executor outside Concord.

The GitHub SDK's response-head/error patterns, Stripe's bounded typed error
adapter, the pagination README, and Concord's runtime-body examples supplied
implementation patterns. `PROVENANCE.md` records the official pages checked on
2026-11-06 and every fixture transformation. All fixtures and the fake state
machine are synthetic. Passing them is **not Google Drive vendor conformance**;
no credentials or live provider calls were used.

Run from the workspace root (all cargo invocations use the required shared cache):

```sh
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-google-drive
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo clippy --locked -p concord-sdk-google-drive --all-targets -- -D warnings
CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo fmt -p concord-sdk-google-drive
```

Optional live verification requires a separately authorized, scoped test Drive
and an injected OAuth token; it is not part of this package's offline tests.
