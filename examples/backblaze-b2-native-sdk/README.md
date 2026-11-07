# Backblaze B2 Native API: a bounded Concord SDK slice

`concord-sdk-backblaze-b2-native` covers two connected workflows of the B2 Native API **v4**:

1. **Authorize and upload**: `b2_authorize_account` with an application key returns the account
   token and the URLs to use (`apiUrl`, `downloadUrl`); `b2_get_upload_url` returns an upload
   URL with its **own** upload token; `b2_upload_file` sends one small file with its exact
   `Content-Length` and its SHA-1 (`X-Bz-Content-Sha1`).
2. **Download and verify**: `b2_download_file_by_id` returns the bytes; the SDK checks them
   against the returned `X-Bz-Content-Sha1` and hands them out only when they match.

Every request is declared with `concord::api!` (`src/wire.rs`). The slice was built and tested
only against local stand-ins: **no live B2 call was made** (see `PROVENANCE.md`).

## Calling it

```rust,no_run
use concord_sdk_backblaze_b2_native::{
    ApplicationKey, BucketId, Client, ContentType, FileName, UploadFile, UploadPolicy,
    http_client_builder,
};

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
// Transport settings (timeouts, proxies, TLS) are yours; keep redirects and reqwest retries off.
let http = http_client_builder()
    .timeout(std::time::Duration::from_secs(60))
    .build()?;
let client = Client::new(http);
let key = ApplicationKey::new("<applicationKeyId>", "<applicationKey>")?;
let session = client.authorize_account(&key).await?;

// Workflow 1: an upload URL (with its own token), then one upload.
let bucket = BucketId::new("<bucketId>")?;
let mut target = session.get_upload_url(&bucket).await?;
let file = UploadFile::bytes(
    FileName::new("reports/2026 summary.txt")?, // sent percent-encoded
    ContentType::new("text/plain")?,             // or ContentType::auto() for `b2/x-auto`
    b"hello".to_vec(),                            // length and SHA-1 computed here
);
let uploaded = session.upload_file(&mut target, file, UploadPolicy::once()).await?;

// Workflow 2: download by ID; the bytes are returned only if their SHA-1 matches.
let download = session
    .download_file_by_id(&uploaded.file.file_id, 16 * 1024 * 1024)
    .await?;
assert_eq!(&download.bytes[..], b"hello");
# Ok(()) }
```

A file can also be a one-shot source whose length and SHA-1 the caller knows
(`UploadFile::stream(name, content_type, length, sha1, stream)`): it is sent once, with
`Content-Length: length`, and is never replayed.

## Hosts and credentials

| Operation | Host | `Authorization` |
|---|---|---|
| `b2_authorize_account` | `https://api.backblazeb2.com` (declared base) | `Basic base64(keyId:applicationKey)` (via `concord-auth`) |
| `b2_get_upload_url` | the issued `apiUrl` | the account token |
| `b2_upload_file` | the issued `uploadUrl`, path and query as issued | the upload token of that URL |
| `b2_download_file_by_id` | the issued `downloadUrl` | the account token |

An issued URL must be `http`/`https` without credentials or fragment, must not downgrade an
`https` issuer to `http`, and (for `apiUrl`/`downloadUrl`) must carry no query; an issued token
must be header text. Otherwise the call fails with `Error::Contract` and nothing is sent there.
The account token is valid for at most 24 hours; the SDK does not re-authorize by itself
(`401 expired_auth_token` from `b2_get_upload_url` or a download means: authorize again).

## Upload semantics: what an attempt proves

B2 documents an upload URL and its token as valid "for 24 hours or until the endpoint rejects an
upload", and a `503` on upload as "call `b2_get_upload_url` again". The SDK therefore:

- **retires** the target's upload URL after every attempt that was not stored, and obtains a new
  one before the next attempt (`UploadTarget::is_usable` tells which state it is in); a stored
  upload keeps the URL for the next file;
- serves one upload per target at a time (`&mut UploadTarget`: B2 refuses a token already in use,
  `400 auth_token_limit`);
- reports every attempt's **evidence** and never claims exactly-once storage:

| `Evidence` | When | May have stored? | Retried with `UploadPolicy::retrying` |
|---|---|---|---|
| `Stored` | 2xx whose file version matches the name, bucket, length and SHA-1 sent | yes (that is the result) | — |
| `StoredUnverified` | 2xx whose body is unreadable or disagrees with what was sent | **yes** | never |
| `Refused` | 4xx | no | `401 bad_auth_token`/`expired_auth_token`, `400 auth_token_limit`, `408 request_timeout` |
| `ServerFailed` | 5xx | **not proven either way** | `503` only |
| `NoAnswer` | no response head (timeout, reset, closed connection) | **not proven either way** | yes |
| `SourceFailed` | a one-shot source ended early or failed: `Content-Length` was never reached | no | never |
| `Abandoned` | in flight when the policy's deadline expired | **not proven either way** | — |
| `NotSent` / `NoUploadUrl` | the request was not built / `b2_get_upload_url` failed | no | `NoUploadUrl` after a `503` or no answer |

`UploadPolicy::once()` (the default) makes one attempt. `UploadPolicy::retrying(n)` permits
sending the file again (`concord-retry`'s `Replay::Permitted`): the caller accepts that B2 may
end up holding more than one version of the file. **A new upload URL alone proves nothing about
earlier attempts**: `Uploaded::possible_duplicates()` counts the earlier attempts that may also
have stored the file, and `UploadError::may_have_stored()` says whether any attempt may have.
Resolving that (listing the bucket's versions of the name) is outside this slice.

**A one-shot source is sent at most once**, whatever the policy: a new upload URL cannot replay a
consumed stream. After a `503` on a streamed upload the error reports `stream_consumed()` and
stops with `StopReason::NotReplayable`; the caller needs a new source. A source that was never
handed to the transport (for example when `b2_get_upload_url` failed first) comes back through
`UploadError::into_file()`; bytes held in memory always do.

```rust,no_run
# use concord_sdk_backblaze_b2_native::*;
# async fn run(session: Session, mut target: UploadTarget, file: UploadFile) {
let policy = UploadPolicy::retrying(std::num::NonZeroUsize::new(3).unwrap());
match session.upload_file(&mut target, file, policy).await {
    Ok(uploaded) if uploaded.possible_duplicates() > 0 => {
        // Stored, and an earlier attempt may have stored another version too.
    }
    Ok(_uploaded) => {}
    Err(failure) if failure.may_have_stored() => {
        // Do not assume the file is absent; do not blindly resend.
    }
    Err(failure) => {
        // Refused or never sent: the file comes back when it can still be sent.
        let _again = failure.into_file();
    }
}
# }
```

## Download verification

`download_file_by_id(file_id, limit)` reads at most `limit` bytes (`ErrorKind::Limit` beyond)
and requires status `200`, a single `X-Bz-File-Id` equal to the requested ID, and a single
`X-Bz-Content-Sha1` that is 40 hex digits and equals the SHA-1 of the bytes. `none` (B2 has no
SHA-1, e.g. a large file) is `ChecksumFailure::Unavailable`; a missing, repeated or malformed
header is refused too. No `Range` is sent, so the whole file is verified. The decoded name,
content type, upload timestamp and `X-Bz-Info-*` values are returned beside the bytes.

## Failures

`Error` is one of:

- `Input`: refused locally, nothing sent (empty IDs or names, a key the Basic credential cannot
  carry, a media type that is not `type/subtype`, a SHA-1 that is not 40 hex digits);
- `Concord`: building, transport, a refused response head, an undecodable or oversized 2xx body
  (`concord::Error` names the operation and keeps the head; `delivery()` says whether the request
  may have reached B2);
- `Provider`: a non-2xx answer. The HTTP status is authoritative; `code()` is the typed B2
  `code` (every code the four pages document, `ErrorCode::Other` for any other) when the body was
  a B2 error document whose `status` matches; a proxy page, an oversized, interrupted or
  contradictory document keeps the status and says why (`unreadable()`,
  `inconsistent_document()`);
- `Contract`: a 2xx that broke a documented rule (issued URL or token, bucket, uploaded file
  version, download status or file ID);
- `Checksum`: a download that does not verify (the bytes are not returned);
- `Deadline`: the upload policy's deadline expired during an attempt.

`upload_file` returns `UploadError`: the last attempt's `Error`, the `UploadReport` of every
attempt, and the file when it can still be sent.

## Redaction

`Debug` and `Display` never show the application key, the Basic credential, the account or
upload token, the account ID (it doubles as the master key ID), an issued upload URL, file
content or B2's free-text `message` (read it with `ProviderError::message()`). Concord redacts
its own output (`#[sensitive]` credential headers); the issued upload URL is reached through
`Prepared::follow`, whose `Debug` withholds a followed URL.

## Transport assumptions

Use `http_client_builder()` (redirects off, reqwest retries off) or an equivalent client: a
followed redirect would make a status answer another request than the upload, and a reqwest
retry would be a repetition the upload report does not count. Timeouts are yours.

## Excluded

Large files and their parts (`b2_start_large_file`, `b2_upload_part`, `absoluteMinimumPartSize`,
`recommendedPartSize`), the SHA-1 sent at the end of the body (`hex_digits_at_end`), lifecycle
rules, the S3-compatible API (`s3ApiUrl`), server-side encryption headers (SSE-B2, SSE-C),
Object Lock (retention, legal hold), custom upload timestamps, custom `X-Bz-Info-*` names other
than `src_last_modified_millis` and the `b2-*` download-header infos, ranged downloads, download
by name, the Partner (`groupsApi`) API, automatic re-authorization, and any claim of
exactly-once storage.

## Tests

```sh
cargo test --locked -p concord-sdk-backblaze-b2-native
```

`tests/protocol.rs` checks request shapes on the wire, every documented error code, malformed /
truncated / oversized / wrong-media bodies, documented-rule violations and redaction;
`tests/workflows.rs` runs both workflows end to end and the required failure. Both run against
`tests/common`, four loopback servers (account, API, upload, download hosts) built on
`examples/testserver`.
