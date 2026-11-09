# concord-sdk-amazon-s3

A bounded Amazon S3 SDK for **general-purpose buckets**, built on Concord (`concord::api!`,
`Prepared`/`Decoder`/`Received`, custom `Decode`/`Incremental` formats) and `concord-pagination`.
Every request is signed with **AWS Signature Version 4** in the `Authorization` header.

This is not the AWS SDK and not a complete S3 client. Its tests run offline against a loopback
fake; they show that the SDK does what AWS's documentation describes, **not** that AWS accepts it
(see [Evidence](#evidence-and-what-it-does-not-prove)).

## Scope

| Operation | Request | Result |
|---|---|---|
| `ListObjectsV2` | `GET /?list-type=2` with `prefix`, `delimiter`, `max-keys`, `start-after`, `continuation-token`, `fetch-owner`, `encoding-type=url` | one page (`ListObjectsV2Output`) or every page (`ListObjectsV2::pages`, a `concord_pagination::Pages`) |
| `HeadObject` | `HEAD /{key}` with `Range`, `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`, `versionId`, `partNumber` | `ObjectHead` (system and `x-amz-meta-*` metadata, validators, request ids) |
| `GetObject` | `GET /{key}` with the same conditions, one `Range`, `versionId`, `partNumber`, the six `response-*` overrides | `GetObjectOutput`: the parsed head and the **unread** body (`ObjectBody`) |
| `PutObject` | `PUT /{key}`, one request, a finite in-memory body, an explicit `Content-Type`, user metadata, `Cache-Control`, `Content-Disposition`, `Content-Encoding`, `Content-Language`, `Expires`, `x-amz-storage-class`, `If-Match`, `If-None-Match: *` | `PutObjectOutput` (ETag, version id, request ids) |

Bucket-wide on a `Bucket` handle: `x-amz-expected-bucket-owner`, `x-amz-request-payer: requester`.

The bucket handle and the operation builders are generated from the Concord declaration
(`src/api.rs`): endpoint and optional values have a `set_*` setter (`maybe_*` takes an
`Option`), documented with the S3 parameter it sends. Required bucket identity is fixed;
`mut signer` permits credential and clock replacement. The endpoint and signer are read through getters. The client's SigV4 context is the
declaration's **send policy** (`send signer;`), so each builder's `send()` is signed after the
request is built. `prepare()` is the declared request **unsigned**; `S3Client::prepare(op)` signs
one without sending it.

Signing: Region, service `s3`, the client's credentials (one request with other credentials:
`s3.clone().with_credentials(c).bucket(..)`), session tokens (`x-amz-security-token`, signed), a
clock (`Clock::system()`, `Clock::fixed(..)` for tests). The payload is always signed
(`x-amz-content-sha256` is the body's SHA-256).

**Excluded:** multipart uploads, presigned URLs, streaming/chunked signatures
(`STREAMING-AWS4-HMAC-SHA256-PAYLOAD`, trailers), `UNSIGNED-PAYLOAD`, SigV4a, directory buckets
(`--x-s3`), path-style addressing (so bucket names with dots), access points and Outposts ARNs,
SSE-C/SSE-KMS request headers, checksums (`x-amz-checksum-*`), tagging, ACLs, object lock, every
other S3 operation and every other AWS service, credential discovery (environment, profiles,
IMDS, STS) and refresh, retries.

## Construction

```rust,no_run
use concord_sdk_amazon_s3::{Bucket, BucketName, Credentials, Region, S3Client, http_client_builder};

fn bucket() -> Result<Bucket, Box<dyn std::error::Error>> {
    // No redirect following, no automatic decompression; add your own timeouts/proxy/TLS.
    let http = http_client_builder().timeout(std::time::Duration::from_secs(30)).build()?;
    let credentials = Credentials::new(
        std::env::var("AWS_ACCESS_KEY_ID")?,
        std::env::var("AWS_SECRET_ACCESS_KEY")?,
    )?;
    // Temporary credentials: .with_session_token(std::env::var("AWS_SESSION_TOKEN")?)?
    let s3 = S3Client::new(http, Region::new("eu-west-3")?, credentials)?; // https://s3.eu-west-3.amazonaws.com
    Ok(s3.bucket(BucketName::new("my-bucket")?)) // my-bucket.s3.eu-west-3.amazonaws.com
}
```

- `S3Client::with_endpoint(url)` replaces the endpoint (another partition, FIPS/dual-stack, an
  S3-compatible server, a test server): `http`/`https`, a domain host (buckets are host labels),
  no path. `with_credentials` and `with_clock` return new clients; like Concord's handles they
  are snapshots, and handles created earlier keep their values.
- The `reqwest::Client` is yours. Build it with `http_client_builder()` or apply its two
  settings: **no redirects** (a `301 PermanentRedirect`/`307` must reach you with
  `x-amz-bucket-region`; a followed redirect would carry a signature made for another host) and
  **no automatic decompression** (object bytes must arrive as stored). Default headers you add
  to the client are not signed: never add `x-amz-*` defaults (S3 requires every `x-amz-*` header
  to be signed).
- `S3Client::sign(&mut request)` signs a native request of an operation this SDK does not
  declare, with the client's context (the target is signed as it is).
- **User metadata** (`x-amz-meta-*`): its header names are data, which the declaration cannot
  place, and the send policy sees only the built request. `PutObject::metadata(m)` is therefore
  the last call of a put, and the result is sent with the client it came from:
  `.metadata(m).send(&s3)` (or `.prepare(&s3)` to inspect the signed request).

## Workflow A: inventory, inspect, read conditionally or partially

```rust,no_run
use concord_sdk_amazon_s3::{Bucket, ByteRange, MaxKeys, ObjectKey, S3Error};

async fn workflow_a(bucket: &Bucket) -> Result<(), Box<dyn std::error::Error>> {
    // Every key under a prefix, over S3's pages (opaque continuation tokens, nothing guessed).
    let objects = bucket
        .list_objects_v2()
        .set_prefix("reports/2026/")
        .set_max_keys(MaxKeys::new(500)?)
        .pages()
        .items()
        .collect_complete()
        .await?; // Err(Incomplete { collected, error }) keeps what was read and where to resume

    let Some(chosen) = objects.iter().max_by_key(|o| o.size.unwrap_or(0)) else {
        return Ok(());
    };
    // Keys can contain spaces and Unicode, e.g. reports/2026/München 東京 résumé.txt.
    let key = ObjectKey::new(chosen.key.clone())?;

    // Metadata only.
    let head = bucket.head_object(key.clone()).send().await?;
    let Some(etag) = head.etag.clone() else {
        return Ok(());
    };

    // A conditional read: 304 when our copy is current, 412 when the object changed.
    match bucket
        .get_object(key.clone())
        .set_if_match(etag)
        .set_range(ByteRange::inclusive(19, 25)?)
        .send()
        .await
    {
        Ok(part) => {
            let range = part.head.content_range; // Some(..) on a 206
            let (head, bytes) = part.bytes(4096).await?; // refuses more than 4096 bytes
            println!("{} bytes of {range:?}, metadata {:?}", bytes.len(), head.metadata);
        }
        Err(S3Error::NotModified(not_modified)) => println!("current: {:?}", not_modified.etag()),
        Err(error) if error.is_precondition_failed() => println!("changed since we listed it"),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
```

## Workflow B: put finite bytes, read them back

```rust,no_run
use concord_sdk_amazon_s3::{AnyObject, Bucket, Bytes, ContentType, Metadata, ObjectKey, S3Client};

async fn workflow_b(s3: &S3Client, bucket: &Bucket) -> Result<(), Box<dyn std::error::Error>> {
    // Sent as uploads/2026/na%C3%AFve%20caf%C3%A9.bin
    let key = ObjectKey::new("uploads/2026/naïve café.bin")?;
    let body = Bytes::from(vec![0u8, 1, 2, 255]);
    let put = bucket
        .put_object(key.clone(), body.clone(), ContentType::new("application/octet-stream")?)
        .set_if_none_match(AnyObject) // If-None-Match: *  (412 when the key exists)
        .metadata(Metadata::new().insert("author", "me")?) // last: sent with the client
        .send(s3)
        .await?;

    let object = bucket.get_object(key).send().await?;
    assert_eq!(object.head.etag, put.etag); // compared as opaque validators
    let (head, bytes) = object.bytes(1 << 20).await?;
    assert_eq!(&bytes[..], &body[..]);
    assert_eq!(head.metadata.get("author"), Some("me"));
    Ok(())
}
```

## Errors

Every call returns `Result<_, S3Error>`. The variant says how far the call got:

| Variant | Sent? | Carries |
|---|---|---|
| `Input` | no | which value and which rule (never the value) |
| `Build` | no | Concord's `Build` error (an invalid header value, a bad path segment) |
| `Signing` | no | reason and header **name** (a header value outside visible ASCII cannot be signed) |
| `Transport` | yes, no response head | Concord's `Transport` error, URL removed |
| `Api` | yes | `ApiError`: status, `ResponseMeta` (`x-amz-request-id`, `x-amz-id-2`, `x-amz-bucket-region`), the `<Error>` document's `Code`, `Message`, `Resource`, `RequestId`, `HostId`, and what happened to the body (`ErrorBody`: parsed, empty — always for `HEAD` —, unrecognized, larger than 64 KiB, unreadable) |
| `NotModified` | yes | `304`: the current `ETag` and `Last-Modified` |
| `Response` | yes, 2xx | Concord's `Metadata` (head refused before the body was read: a non-XML listing, a malformed object header, a `206` without one valid `Content-Range`, `multipart/byteranges`) or `Decode` (invalid XML, an `<Error>` inside a 200) error, with `ResponseMeta` |
| `Protocol` | yes | a broken contract: truncated page without token, repeated token, a `206` answering another range |
| `Body` | yes | reading an object body: transport failure, truncated, or over the caller's limit; bytes received and expected |

Helpers: `status()`, `code()`, `request_id()`, `meta()`, `is_not_found()`,
`is_precondition_failed()`, `is_not_modified()`, `is_before_send()`. `Display`/`Debug` print the
operation (`S3Api.bucket.get_object`), status, code and request ids; never credentials,
signatures, URLs, header values or S3's free-text `Message`/`Resource` (read those through
`ApiError`).

## Completion and partial outcomes

- **Listing.** `pages()` returns `Ok(None)` only after a page with `IsTruncated` false. A cap
  (`max_pages`) reached while S3 has more is `PagesError::Bounded { resume }`; a failed page is
  `PagesError::Provider { error, failed }`; both end the traversal, and `collect_complete()`
  returns the collected prefix with the error. Resume with
  `list_objects_v2().set_continuation_token(token).pages()` on the same request. A listing is not a
  snapshot: objects written meanwhile may or may not appear.
- **Object bodies.** `GetObject`'s `send()` returns once the head is checked; the body is read by
  `ObjectBody::chunk()` or `collect(limit)` (refuses before reading when `Content-Length`
  exceeds the limit). `chunk()` returns `Ok(None)` after exactly `Content-Length` bytes, and
  also on every call after an error: keep the first error. `ObjectBody::completion()` is
  `BodyCompletion::EndOfInput` only at the verified end (`Failed(kind)` after an error,
  `Reading` before the end), as the Concord readers' `completion()`. A failure reports the bytes
  received; a partial body is never returned as complete.
- **Ranges.** One range per request (S3 serves no multi-range `GET`). A `206` must carry one
  valid `Content-Range` that answers the requested range (S3 may shorten a range at the object's
  end); a `200` to a ranged request means the whole object was sent (`head.is_partial()` false).
- **Writes.** `PutObject` is one request; S3 documents that it never stores a partial object. A
  failed write may still have been applied if the response was lost (`Transport`): whether to
  retry, and with which condition (`set_if_none_match(AnyObject)`, `set_if_match`), is yours.
- **Cancellation.** Dropping a pending `send()` or an `ObjectBody` stops the transfer and closes
  its connection; nothing continues in the background and nothing is retried. Dropping a pending
  `chunk()` loses nothing: the next call continues the body. A request already transmitted may
  still have been executed by S3.
- **ETags** are opaque validators for `If-Match`/`If-None-Match` and equality checks. They are not
  content checksums (S3 documents exceptions: multipart uploads, SSE-KMS, directory buckets); this
  SDK never derives a digest from them.

## Limitations

- Virtual-hosted-style addressing only: bucket names must be DNS labels without dots, and the
  endpoint must be a domain name (for a local emulator, map the bucket host names, as the tests do
  with `reqwest::ClientBuilder::resolve`).
- Object keys with a `.` or `..` path segment are refused (URL parsers remove them from a request
  path; AWS recommends against them). `/` in a key is sent unencoded, every other byte outside
  `A-Z a-z 0-9 - . _ ~` percent-encoded once.
- Header values must be visible ASCII to be signed (SigV4 signs header text); user metadata values
  are restricted to US-ASCII (S3 rewrites other values with RFC 2047, which would not round-trip);
  user metadata is limited to 2 KB.
- `ListObjectsV2Output::key_count` is reported as S3 sends it, not checked against the page.
  `LastModified` in listings is kept as S3's ISO 8601 text. `Last-Modified` headers are parsed as
  HTTP dates, tolerating a wrong day name and a one-digit day (both appear in AWS's examples).
- Bodies are in memory for `PutObject` (the payload hash needs the bytes); there is no streaming
  upload.
- No retries, no credential providers, no clock-skew correction (`RequestTimeTooSkewed` is an
  `Api` error).

## Evidence, and what it does not prove

- `cargo test -p concord-sdk-amazon-s3`: unit tests, `tests/protocol.rs` (wire-level requests,
  documented and synthetic responses, negative cases, perturbations, cancellation) and
  `tests/workflows.rs` (workflows A and B end to end). The maintenance scenario changes a
  Unicode key and a signed range independently on loopback, compares their canonical requests
  and signatures, and rechecks both list pages and an unrelated HEAD. Unknown elements in both
  listing fixtures are ignored while expected fields still decode.
- The SDK signs with `aws-sigv4`, AWS's own Rust signer. The tests check it against **AWS's four
  published signing examples** (`fixtures/sigv4/`, documentation example key only): the GET
  example is reproduced through the SDK's own `GetObject` request; the PUT, lifecycle and list
  examples through the public signer. An **independent** verifier written from the documented
  algorithm (`tests/common/mod.rs`) recomputes the canonical request, the string to sign and the
  signature from what reached the wire, and the fake S3 refuses any request it cannot verify.
- Fixtures are documented examples or hand-authored from the documentation (`PROVENANCE.md`).
  A fake server, however faithful to the documentation, is **not vendor conformance**: nothing
  here was sent to AWS, and an S3-compatible emulator would only add another implementation's
  opinion. A live check needs a bucket, IAM credentials and network access, none of which the
  tests use.

`FRICTION.md` records what Concord made easy and where this SDK had to work around it.
