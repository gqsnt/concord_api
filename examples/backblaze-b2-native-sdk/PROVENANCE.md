# Sources and fixture provenance

Target: **Backblaze B2 Native API, version v4** (`/b2api/v4/...`). Every source was read on
**2026-11-07** (UTC). **Live provider calls: NOT_RUN.** No B2 account, application key, SDK
recording or captured provider response was used; every test runs against loopback stand-ins.
Fixture assertions establish this SDK's behaviour against the documentation as read, not B2
conformance.

## Official pages

Each page was downloaded once with `curl` (HTML), and its visible text extracted for reading.
Each page states "Updated on 11 Sep 2026". The SHA-256 of the downloaded HTML and of the
extracted text pin what was read (the HTML may embed per-request markup; the text is the stable
pin).

| Key | Page | Used for |
|---|---|---|
| A | <https://www.backblaze.com/apidocs/b2-authorize-account> | `GET /b2api/v4/b2_authorize_account`; Basic `applicationKeyId:applicationKey`; the response schema and example (`accountId`, `apiInfo.storageApi.{apiUrl, downloadUrl, allowed{buckets, capabilities, namePrefix}}`, `authorizationToken` "valid for at most 24 hours", `applicationKeyExpirationTimestamp`); errors 400 `bad_bucket_id`/`bad_request`, 401 `unauthorized`/`unsupported`, 403 `transaction_cap_exceeded`; the error document `{status, code, message}` ("status ... always matches the status in the HTTP response") and its example |
| G | <https://www.backblaze.com/apidocs/b2-get-upload-url> | `GET /b2api/v4/b2_get_upload_url?bucketId=`; account token with `writeFiles`; response `bucketId`, `uploadUrl`, `authorizationToken` and their examples; "valid for 24 hours or until the endpoint rejects an upload"; errors 400 `bad_bucket_id`/`bad_request`, 401 `bad_auth_token`/`expired_auth_token`/`unauthorized`, 403 `storage_cap_exceeded`, 503 `service_unavailable` |
| U | <https://www.backblaze.com/apidocs/b2-upload-file> | `POST` to the issued upload URL (example path form `.../b2_upload_file/<bucket>/<cvt>`); headers `Authorization` (upload token), `X-Bz-File-Name` ("percent-encoded UTF-8 ... spaces should be replaced with %20"), `Content-Type` (`b2/x-auto`), `Content-Length` ("chunked transfer encoding ... is not supported"), `X-Bz-Content-Sha1`, `X-Bz-Info-src_last_modified_millis`; the file-version schema and field examples; errors 400 `auth_token_limit`/`bad_bucket_id`/`bad_request`/`custom_timestamp_invalid`/`custom_timestamp_not_allowed`, 401 `bad_auth_token`/`expired_auth_token`/`unauthorized`, 403 `cap_exceeded`, 405 `method_not_allowed`, 408 `request_timeout`, 411 `length_required`, 415 `unsupported_media_type`, 503 `service_unavailable` ("Call b2_get_upload_url again") |
| D | <https://www.backblaze.com/apidocs/b2-download-file-by-id> | `GET {downloadUrl}/b2api/v4/b2_download_file_by_id?fileId=`; account token (`readFiles`); response headers `Content-Length`, `Content-Type`, `X-Bz-File-Id`, `X-Bz-File-Name`, `X-Bz-Content-Sha1`, `X-Bz-Info-*`, `X-Bz-Upload-Timestamp`; "you should check the SHA1 of the data you receive against the SHA1 returned in the X-Bz-Content-Sha1 header"; `Range` and 206 semantics (not used); errors 400 `bad_bucket_id`/`bad_request`, 401 `bad_auth_token`/`expired_auth_token`/`unauthorized`, 403 `access_denied`/`download_cap_exceeded`/`transaction_cap_exceeded`, 404 `not_found`, 416 `range_not_satisfiable` |

```text
key  HTML sha256                                                       text sha256
A    41a255e46a79200ca5cfb54988e4c57ba3a55bff4e212f53420dd63a41363d9c  cef8e718c5e0928aba96a57b06adf1311adfbbc1e7d753096cc15729afbb3c6a
G    5f00624a688e25383debd0c09aba0eace95f99be84d74d74d6ac7c47f7695f70  7314aa170dfd0009d2bd7c33d2938270bfc93e6f2993efcb74bb427da514e03a
U    06c4186b19918c59e3684436c9c7b2afa9ba1104c1b7824fe8ef818acd27e6e6  e7107ee9a238bbc53368da7b5524b8afaef2c18b2f5e943f413ce7460e0f8594
D    38a724b9f70de4d1fc1e1b9a9fd099e9add035e5fd5c65b24f2ecd767a08e03d  a239a43253e00814fd2a562fa8991bf783c0b72e9a107fc7e9a3d543cff34cf5
```

No other page was read (in particular not B2's "Files", "String Encoding", "Uploading" or
integration-checklist pages that the four pages link to); rules that live only there are not
implemented, and the gaps are listed below.

## What the pages leave open, and what the SDK decided

| Point | Documentation (key) | SDK decision |
|---|---|---|
| `contentLength` type | schema type `string`, example `"46"` (U) | accept a decimal string or a JSON number; nothing else (`+63`, `63.0` are refused) |
| `contentMd5` example | 40 hex digits for a field described as 32-digit MD5 (U) | kept as an optional string, not validated |
| `required` list of the authorize schema | names fields that are not properties (A) | decode the documented properties; `allowed`, `apiUrl`, `downloadUrl`, `authorizationToken`, `accountId` required |
| 401/403 bodies | tables without a content schema (A, G, U, D) | read every non-2xx body as the `{status, code, message}` document when it is one; the status stays authoritative otherwise |
| `uploadUrl` form | query form in G's example, path form in U's text | treated as opaque; followed verbatim (path and query) |
| `X-Bz-File-Name` encoding set | "percent-encoded UTF-8", spaces as `%20` (U); response header "as if it were a URL parameter" (D) | send every byte except `A-Z a-z 0-9 - . _ ~ /` percent-encoded (more encoding than needed decodes the same); decode the response header with `%XX` only, `+` kept literal |
| retry after 503 | "Call b2_get_upload_url again" (U); whether the attempt stored nothing is not stated | new upload URL, and the attempt is reported as **not proven either way** |
| retry after 401 `bad_auth_token`/`expired_auth_token` on upload | "Call b2_get_upload_url again to get a new one" (U) | retried at a new URL when replay is permitted |
| retry after 400 `auth_token_limit`, 408 `request_timeout` | "already being used"; "timed out reading the uploaded file" (U) | SDK decision: retried at a new URL when replay is permitted (a new URL has its own token) |
| `X-Bz-Content-Sha1: none` | U: large files have `contentSha1` "none" | download verification fails with `ChecksumFailure::Unavailable` |
| file name rules, the 7,000-byte name+info limit | U mentions the limit; the rules live on the Files page (not read) | only "not empty" is checked locally; B2's `400 bad_request` is surfaced |
| custom `X-Bz-Info-*` names | U: "the custom field name ... is converted to lower case"; name grammar on the Files page (not read) | not sent (excluded); read back on download |

## Fixtures

All files are UTF-8 with a final LF unless stated. "Assembled" means built by hand from the
field examples of the cited page; "synthetic" means authored for the test.

| File | Kind | Derivation |
|---|---|---|
| `authorize_account.json` | documented example | A's 200 example, verbatim (extracted from the page text, validated as JSON). The stand-in replaces `apiUrl` and `downloadUrl` with its loopback hosts at run time |
| `get_upload_url.json` | assembled | G's field examples for `bucketId`, `uploadUrl` (query form) and `authorizationToken`, unchanged. The stand-in replaces the URL and token per issued URL |
| `upload_file.json` | assembled | U's field examples for `accountId`, `bucketId`, `fileId`, `contentType`, `fileInfo`, `fileRetention`, `legalHold`, `serverSideEncryption`, `uploadTimestamp`; **changed:** `action` `"upload"` (the example is `"start"`), `fileName` `hello-b2.txt`, `contentLength` `"63"` and `contentSha1` of `hello-b2.txt`, `contentMd5` `null`. The stand-in sets `fileId`, `fileName`, `bucketId`, `contentLength`, `contentSha1`, `contentType` from what it received |
| `error_document.json` | documented example | the error schema's examples in A/G/U/D (`400`, `invalid_bucket_name`, "bucket name is too long"), assembled into one document |
| `errors.json` | assembled table | every (operation, status, code) row of the four pages' error tables; the stand-in answers each with a synthetic `{status, code, message: "synthetic <code>"}` |
| `credentials.json` | synthetic | key ID `synthetic-key-id`, key `synthetic-application-key`; `authorization` computed with `printf '%s' 'synthetic-key-id:synthetic-application-key' \| base64 -w0` (coreutils), not by the SDK |
| `hello-b2.txt` | synthetic | 63 bytes of text |
| `hello-b2.json` | synthetic, computed | `length` from `wc -c`; `sha1` from `sha1sum hello-b2.txt`; `tamperedSha1` from `printf 'tampered!' \| sha1sum`; `emptySha1` from `sha1sum /dev/null`; `encodedNameHeader` from Python's `urllib.parse.quote('photos/2026 trip/café+notes.txt')` (unreserved characters and `/` kept); none from the SDK |

```text
71b6190b29074208712aaff40d383131ff350f544ff89b5482483e49059a81dc  authorize_account.json
5b7536ebefc8a2d511ff418a8564cde4f49a7f5c3e1e40a22ed496c073fb3b36  credentials.json
5e968fc2956da91ecbc74fabb6eb105660d352e3e3f105298721fbb5e9771456  error_document.json
f119e4f1d62beb5ffa07075cbc992144672d7d19e72989c68507cd19555015f0  errors.json
6e15d4fb31e1afd95bea843bc269b1af9c174576375be0fa162835d7572c7d1e  get_upload_url.json
d5d21f2aca989994818235b7c70b5f20b657af86da38edcd0bbe28e44fc83709  hello-b2.json
96dc79771becc579010616a79b9c41b0c2dbf1d29746f0e92128865a5ec32c6d  hello-b2.txt
5bd5522971e6ea3676091cd018029b1b96b1ae217387d997079070cc6fd33053  upload_file.json
```

The stand-in (`tests/common/mod.rs`) is synthetic too: four loopback hosts sharing an in-memory
bucket. It enforces the documented request rules it can check without the SDK's code (`POST`,
a live upload token, `Content-Length` equal to the body and no chunked transfer, the three
required headers) and retires an upload token after a rejection, as G states. It decodes
`X-Bz-File-Name` with its own `%XX` decoder and never computes a SHA-1: it stores and returns the
`X-Bz-Content-Sha1` it received, and tests compare against `hello-b2.json`.

## Dependencies

Already in `Cargo.lock` before this package: `concord`, `concord-auth` (and its `base64`),
`concord-retry` (feature `tokio`: `tokio` with `time`), `serde`, `percent-encoding`,
`futures-core`, `http-body`, `mime`; dev: `concord-example-testserver`, `serde_json`, `tokio`.

New: **`sha1_smol` 1.0.1**, BSD-3-Clause, no dependencies (its optional `serde` feature is not
enabled). Why: SHA-1 is required for `X-Bz-Content-Sha1` (U) and for verifying downloads (D),
and no SHA-1 implementation was in the lock. RustCrypto's `sha1` 0.10.7 (MIT OR Apache-2.0,
available offline) needs `digest` 0.10 and its `block-buffer` 0.10 / `crypto-common` 0.1 /
`generic-array` 0.14 / `cpufeatures` 0.2, none of which is in the lock (it has `digest` 0.11.3);
`sha1` 0.11 was not available offline. `sha1_smol` adds exactly one package. The build that
wrote the lock ran with `--offline`; the lock diff adds only this package and
`concord-sdk-backblaze-b2-native`, and no other package changed version.

## Examples and crate sources consulted

Recorded in `FRICTION.md` ("Examples consulted").
