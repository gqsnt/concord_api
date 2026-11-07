# Fresh consumer: SDK44 Backblaze B2 Native

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application requests will use
`concord-example-testserver` loopback fixtures and synthetic credentials.

## Predictions

Written before application code or any caller test was run, after reading the SDK README
and generated public rustdoc. No SDK implementation, existing SDK tests, FRICTION.md or
PROVENANCE.md has been read.

### Workflow 1: authorize, obtain upload URL, upload

1. Build the transport with `http_client_builder()` (redirects/retries disabled), set a
   timeout and disable proxies for loopback, then construct `Client` with
   `with_base_url(account_fixture.url())`. `ApplicationKey::new` rejects credentials that
   cannot be represented as Basic auth, before any request.
2. `client.authorize_account(&key)` will send GET
   `/b2api/v4/b2_authorize_account`, no query/body, and
   `Authorization: Basic base64(keyId:applicationKey)`. It returns a `Session` with
   public account restrictions/URLs and a privately held account token. The v4 fixture
   will supply `apiInfo.storageApi.apiUrl`, `downloadUrl`, `allowed`, and a token.
3. `session.get_upload_url(&bucket)` will send GET
   `/b2api/v4/b2_get_upload_url?bucketId=<bucket>` on the issued API origin, no body,
   and `Authorization: <account token>`. GET is my prediction from the official page;
   that page also permits POST with a JSON body. It returns an initially usable
   `UploadTarget` bound to that bucket, with its own token and issued URL.
4. `UploadFile::bytes(name, type, bytes).with_last_modified_millis(...)` computes the
   exact length and SHA-1. `session.upload_file(&mut target, file, UploadPolicy::once())`
   will POST raw bytes to the issued upload path **and query unchanged**, using the upload
   token (no Basic or account token), `Content-Length`, `Content-Type`, percent-encoded
   UTF-8 `X-Bz-File-Name`, `X-Bz-Content-Sha1`, and
   `X-Bz-Info-src_last_modified_millis`. No chunked transfer. A matching 200 JSON file
   version returns `Uploaded`, one `Stored` attempt, zero possible duplicates, and leaves
   the target usable. Completion means a verified stored version for that attempt.

Failures and partial results: non-2xx answers retain authoritative HTTP status and a typed
provider code when a consistent B2 error document is available. Unknown codes survive as
`Other`; malformed/contradictory error documents retain status without inventing a code.
Malformed success JSON is `Error::Concord`; an inconsistent file version is `Contract`.
Both are `StoredUnverified` and may have stored, so they must not be replayed automatically.
Local input errors send nothing. A 401 account-token expiry requires caller reauthorization.
Unusable issued URLs/tokens are `Contract` errors before credentials are sent to them.

Required failure: a fully consumed one-shot `UploadFile::stream` receiving 503 will have
one `ServerFailed` attempt, `may_have_stored() == true`, `stream_consumed() == true`,
`StopReason::NotReplayable`, no recoverable file and a retired target even under
`UploadPolicy::retrying(3)`. Explicitly obtaining a fresh upload URL cannot resurrect that
source or change the earlier evidence. By comparison, a caller-authorized in-memory replay
after 503 can obtain a fresh URL and succeed, but returns two attempts and one possible
duplicate. A later failed refresh must preserve earlier uncertain attempts in its report.

### Workflow 2: download the uploaded ID and verify

First complete the same authorization/upload chain, then call
`session.download_file_by_id(&uploaded.file.file_id, limit)`. I predict GET
`/b2api/v4/b2_download_file_by_id?fileId=<encoded ID>` on the issued download origin,
`Authorization: <account token>`, an empty body and no `Range`. A 200 raw response with
one matching `X-Bz-File-Id` and one matching 40-hex `X-Bz-Content-Sha1` returns the complete
verified bytes and metadata (decoded name, content type, upload timestamp, file info).
I will independently compare the SHA-1 to a fixed known digest.

Failures: a corrupted body or missing/malformed/repeated checksum header yields
`Error::Checksum` with no downloadable partial bytes; `none` is
`ChecksumFailure::Unavailable`. A wrong file ID or non-200 successful status is `Contract`.
A body exceeding the caller's bound is `Concord` with `ErrorKind::Limit`; an interrupted
body is also `Concord`, with response metadata retained. Non-2xx JSON, HTML/proxy bodies,
or contradictory documents remain `Provider` with the actual HTTP status. No retry or
reauthorization is automatic. A failed verification must not invalidate the session or
make an earlier successful download inaccessible.

### Pagination, metadata, secrets

Neither workflow is paginated. There is no next-page token or iterator exhaustion:
each method completes once; upload reports describe finite attempts and their stop reason.
I expect operation/status/response headers to remain inspectable on errors. Upload errors
retain the report and recoverable in-memory file even when the last refresh failed.
SDK Debug/Display output must withhold the key, Basic value, account/upload tokens,
account ID, issued upload URL, contents, and provider free-text message. Explicit credential,
URL, message and byte accessors are intentionally unredacted; fixture recordings are also raw.

## Documentation and build observations before code

- Read only the SDK `README.md` and public pages in
  `target/doc/concord_sdk_backblaze_b2_native/` (crate index, `Client`, `Session`,
  upload file/policy/target/report/error/attempt/result types so far).
- Read root/package Cargo manifests and `rust-toolchain.toml` as build metadata.
  Workspace membership was already present; the initial working tree was clean.
  No AGENTS.md was found inside the worktree. No parent campaign files were opened.
- Ran `CARGO_BUILD_JOBS=4 cargo doc --offline -p concord-sdk-backblaze-b2-native --no-deps`
  successfully as the first, unlocked Cargo command. Default `target/` was used;
  `CARGO_TARGET_DIR` was unset. No lockfile edit was needed.
- Generated testserver public rustdoc with
  `CARGO_BUILD_JOBS=4 cargo doc --offline --locked -p concord-example-testserver --no-deps`
  and read its public lifecycle, `TestServer`, `Recorded`, and `Reply` documentation.
- Exact official pages read on **2026-11-07** (read-only research, no API calls):
  [b2_authorize_account](https://www.backblaze.com/apidocs/b2-authorize-account),
  [b2_get_upload_url](https://www.backblaze.com/apidocs/b2-get-upload-url),
  [b2_upload_file](https://www.backblaze.com/apidocs/b2-upload-file), and
  [b2_download_file_by_id](https://www.backblaze.com/apidocs/b2-download-file-by-id).
  These support Basic auth and issued origins, the distinct upload token, exact-length
  raw upload and checksum headers, refresh on rejection/503, and download verification.

## First working call

The first caller run was
`CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-backblaze-b2-native --test first_use -- --nocapture`.
It compiled without warnings or missing imports. Both tests successfully called
`Client::authorize_account`, checked the account/issued origins, obtained an upload URL,
and received a matching stored file version. The download task also received verified bytes
and checked file metadata before its first assertion failure. No SDK source or existing
test was needed. The run had 0 passing tests because consumer assertions then failed (below).

The setup took the README call sequence, `Client::with_base_url` from public rustdoc,
four `TestServer::start` origins, synthetic Basic/account/upload credentials, and v4 JSON
fixtures assembled from the official pages. All transport builders use `no_proxy()`,
the SDK's redirect/retry settings, and a three-second timeout. No advanced send API was used.

## Friction incidents (in order)

1. The SDK's `--no-deps` rustdoc shows `Bytes`, `Stream` and `ResponseMeta` in signatures
   without useful dependency links. Generated/read Concord and concord-retry public
   rustdoc to locate `concord::bytes::Bytes`, metadata accessors and the retry stop reason.
   A tiny caller-owned `futures_core::Stream` avoids adding a stream utility crate.
   No imports proved unnecessary and no dependency was added.
2. First test run: expected response operation names to end in provider names such as
   `b2_download_file_by_id`; the name assertion failed. Added a diagnostic using the
   public `ResponseMeta::operation()` accessor to see the actual generated identity.
   The initial generic assertion did not print the observed name; this was a caller-test
   diagnostic gap. No SDK implementation or existing-test lookup was made.
3. Same run (the two tests ran concurrently): expected `/` to be percent-encoded in
   `X-Bz-File-Name`; observed `reports/2026%20r%C3%A9sum%C3%A9.txt`. The README says
   percent-encoded but does not specify the safe set. Adjusted the wire expectation to
   preserve `/`; spaces and UTF-8 still encode.
4. Second run printed `Operation(GET B2Wire.download.download_file_by_id)` and
   `Operation(POST B2Wire.upload.upload_file)`. These identities retain Concord's fixed
   client → scope → endpoint hierarchy, while using caller method names without `b2_`.
   Corrected assertions to check the caller endpoint suffix, declared method, and endpoint
   identity. The API refresh metadata subsequently matched `get_upload_url` as well.

The third consumer run passed both tasks. No further friction, source lookup, compiler
error, surprising completion, permission request, or dependency addition was needed.
Follow-up public rustdoc reads covered the account/file/download models, errors/evidence,
SHA-1/ID accessors, Concord's `Error`, `ErrorKind`, `Delivery`, `Operation`,
`ResponseMeta`, retry `Report`/`StopReason`, and testserver `Body`/`Cleanup`.

## Predictions compared with observations

| Prediction | Observed in the independent caller tests |
| --- | --- |
| Local input errors send nothing | Invalid Basic key ID, empty bucket/name, invalid content type and invalid SHA-1 were rejected before any account request. |
| GET authorize with Basic, no query/body | Matched, including an independently encoded exact Basic header. v4 account restrictions and separate API/download origins were accessible. |
| GET upload-URL with `bucketId` query and account token | Matched. No JSON body or unexpected reauthorization. |
| POST the issued upload path/query with the upload token | Matched across six upload requests, including the escaped query. Account credentials were not substituted for the per-slot upload token. |
| Exact length, SHA-1, raw bytes, file info | Matched: six bytes, fixed SHA-1 `f572d396fae9206628714fb2ce00f72e94f2258f`, `text/plain`, last-modified metadata, no chunked transfer. |
| Percent-encoded file name | Spaces and UTF-8 encoded as expected; the initial assertion's assumption that `/` would also encode was wrong. The final exact wire expectation preserves it. |
| A matching file version completes once | Matched: one `Stored` attempt, `Done`, zero unresolved earlier attempts and a still-usable target. |
| Stream + 503 cannot replay | Matched: all six source bytes were handed out, one `ServerFailed` attempt with status/code, `NotReplayable`, consumed source, retired URL, possible storage and no recoverable file. No futile automatic URL refresh occurred. |
| A fresh URL cannot restore the stream or resolve prior storage | Matched: explicitly fetched a different usable target, observed no additional upload, and confirmed the retained failure was still uncertain and had no source to return. |
| Permitted byte replay can leave duplicates | Matched: 503 then fresh URL + 200, two upload requests, one unresolved earlier attempt/possible duplicate and `Done`. |
| Failed refresh retains partial results | Matched: 503 followed by 401 `expired_auth_token` retained `ServerFailed` then `NoUploadUrl`, one actual upload, one unresolved prior write, and the recoverable byte file. The account was not reauthorized. |
| Malformed 2xx upload JSON may already have stored | Matched: `Concord(Decode)`, `Delivery::Answered`, one `StoredUnverified`, `ClassifierStop`, possible storage and recoverable bytes, with no replay. |
| GET download by the uploaded ID using the account token | Matched across 15 calls, with the exact decoded `fileId` query, no body and no `Range`. |
| Return complete verified data plus metadata | Matched the fixed digest, bytes, ID, name, content length/type, timestamp and decoded info values. Response status, request ID and declared operation remained available. An exact six-byte limit succeeded. |
| Withhold failed checksum results | Corrupt content produced expected/actual digest mismatch; missing, repeated, malformed and `none` checksum headers produced `Missing`, `Missing`, `Malformed` and `Unavailable`. Only errors, never partial downloadable bytes, were returned. |
| Wrong ID/non-200 success is a contract failure | Matched for another file ID and HTTP 206. |
| Bounded/interrupted downloads retain errors and metadata | A five-byte bound on six bytes yielded `Concord(Limit)` with answered delivery and retained head. A three-byte prefix followed by a closed chunked body yielded `Concord(Transport)` with a retained head. |
| Provider status/code/document handling | Matched for typed 404 `not_found`, unknown 403 code via `Other`, HTML 503 with `unreadable()`, and contradictory JSON whose HTTP 404 stayed authoritative while `code()` was absent. |
| Operation identity | Available as predicted, but the initial provider-name suffix assumption was wrong: it is the generated `B2Wire.<scope>.<caller_method>` identity. |
| Completion, pagination and earlier results | No pagination or hidden retries occurred: exact upload/download request counts and finite reports were asserted. Previously verified bytes survived later errors and a subsequent download still succeeded. |
| Secrets | SDK Debug/Display checks withheld fixture key/Basic value, account ID/token, upload token/URL, file content and provider message. Explicit account/message/URL accessors intentionally exposed their documented values. |

Documented predictions not independently exercised here: malformed or oversized provider JSON,
unusable issued URL/token validation, an inconsistent *well-formed* uploaded file version,
and an unconsumed stream returned after an initial URL acquisition failure. These remain
documentation-based expectations rather than claims of new consumer-test coverage. The
package's existing suites were executed later, without opening their files.

## Validation and change boundary

- Third targeted consumer run: **2 passed, 0 failed**.
- Final command:
  `CARGO_BUILD_JOBS=4 cargo test --offline --locked -p concord-sdk-backblaze-b2-native`.
  Passed **39 runtime tests** (4 unit, 2 new consumer, 18 protocol, 15 workflow) and
  **3 doctests** (including the one-shot move/compile-fail example). No warnings.
- Formatted only the new caller test with `rustfmt --edition 2024`.
- Every Cargo command used four build jobs, default worktree `target/`, and ran alone.
  Only the first documentation build omitted `--locked`; all commands worked offline.
- Added only `FIRST_USE.md` and `tests/first_use.rs`. The workspace member and lock entry
  already existed. `Cargo.toml`, `Cargo.lock`, all SDK implementation files, shared crates,
  checks, scripts and other examples are unchanged. No new crate or version change needs
  a provenance justification. No commit was attempted.
- No shared-core change is needed to complete these calls. Suggested future SDK documentation
  improvements: show the exact filename safe set, show the metadata operation identities,
  and include the imports for the stream and response metadata types. These are recorded
  suggestions only; the SDK source and existing documentation were left intact.
- Live provider calls: **NOT_RUN**. Official documentation was read through the four exact
  links above; all workflow traffic used local fixtures.

## Assistance label

**docs-only**. Never opened the SDK's `src/`, its pre-existing `tests/`, `FRICTION.md` or
`PROVENANCE.md`. Existing tests/doctests were run after the first working call; their names
and diagnostics were observed, but their source was not used to implement these callers.
