# Provenance

Where the SDK's contract, its fixtures and its test vectors come from, what was changed, and what
is synthetic. Nothing in this crate was obtained from a live AWS endpoint: no request was sent to
AWS and no AWS credentials were used.

## Official sources (pinned)

Retrieved on **2026-11-06** between 01:18 and 01:55 UTC with `curl` over HTTPS (HTML), read as
text, then re-fetched once at 01:55 UTC: every page's SHA-256 was identical on both fetches. AWS
documentation pages carry no document version; the pin is the server's `Last-Modified`, its
`ETag` and the SHA-256 of the HTML body. The S3 REST API version these pages describe is
`2006-03-01` (the namespace `http://s3.amazonaws.com/doc/2006-03-01/` of the XML documents).

| Page | Last-Modified | ETag | SHA-256 of the HTML |
|---|---|---|---|
| [API_ListObjectsV2](https://docs.aws.amazon.com/AmazonS3/latest/API/API_ListObjectsV2.html) | Thu, 05 Nov 2026 23:34:57 GMT | `"5bcZzWNi9D56b2TsSwObEIKQJes="` | `d0a125cbb8140b05a0a7bfaee34e2d303e5f5852248af44c9d228e8f2709cc0b` |
| [API_HeadObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html) | Thu, 05 Nov 2026 23:35:01 GMT | `"b+cdvr1DhoKmfwm+P7MPb5t1Rsc="` | `e61c2f87bb93d7164d550c6285a2489ecf5aade45e502c37e1d65fa83b357369` |
| [API_GetObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_GetObject.html) | Thu, 05 Nov 2026 23:35:04 GMT | `"/nPxelH6FA8q2cbGrQR1ihbVdoU="` | `a099d0dfcb408f053541f6dd0226af1adc8ff76b053f4f97bd61a1096958a36d` |
| [API_PutObject](https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html) | Thu, 05 Nov 2026 23:34:55 GMT | `"UShWqUDuM6GHpLKP2nwz+VqGmjE="` | `97d49f3aa801ed7f17c535db3834a165df81ce10ab7c4fcc32160d75c488f876` |
| [sigv4-auth-using-authorization-header](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sigv4-auth-using-authorization-header.html) | Thu, 05 Nov 2026 23:35:57 GMT | `"MKI/xB1xGq9fks/uhdjKk8G3oBY="` | `4454faa779a6a995a6da2bf3ee7e2eb1b21bf3a19eff4658ddfe236ad2a64fba` |

Pages reached from those (same retrieval):

| Page | Used for | Last-Modified | SHA-256 of the HTML |
|---|---|---|---|
| [sig-v4-header-based-auth](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/sig-v4-header-based-auth.html) ("Signature Calculations for the Authorization Header: Transferring Payload in a Single Chunk", linked from the page above) | `UriEncode`, canonical request, string to sign, signing key, **the four signing examples** | Thu, 05 Nov 2026 23:35:57 GMT | `323cc97fffdf79a79ac5620849a79f54a63e771d1c2f628680470d6378ae0092` |
| [ErrorResponses](https://docs.aws.amazon.com/AmazonS3/latest/developerguide/ErrorResponses.html) | the REST `<Error>` document, error codes and statuses | Thu, 05 Nov 2026 23:35:57 GMT | `6753ef344af483d918d912a99f6802aebc820c5a9aa6a45e44dc22b6082bb49d` |
| [bucketnamingrules](https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html) | `BucketName` rules | Thu, 05 Nov 2026 23:36:09 GMT | `4e5730445f629b162666e4f28785fb9fa44e72bc67549addbd8c2886686de905` |
| [object-keys](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-keys.html) | key length, period-only segments | Thu, 05 Nov 2026 23:36:13 GMT | `94044adebfe5613231960873cbe38285a3c7612a80b9a72741f671d860216ca5` |
| [VirtualHosting](https://docs.aws.amazon.com/AmazonS3/latest/userguide/VirtualHosting.html) | virtual-hosted style, dots and the wildcard certificate, path-style status | Thu, 05 Nov 2026 23:36:09 GMT | `868fab6085f2d3d19fa52d4567a65b38e50fd834393f877d1f2f83e73d1d3227` |
| [UsingMetadata](https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html) | `x-amz-meta-*`, US-ASCII, RFC 2047, 2 KB limit | Thu, 05 Nov 2026 23:36:01 GMT | `798248ca44d960819a04a2173302e1015db3f4d88ede2ce03b2c41971ff71514` |

The raw HTML was kept in the session's scratch directory only; it is not part of this crate.

## Signing example used

The SigV4 vectors are AWS's own, from the "Examples: Signature Calculations" section of the
single-chunk page: access key `AKIAIOSFODNN7EXAMPLE`, secret
`wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY` (published by AWS as an example; not a working key),
`us-east-1`, `s3`, `20130524T000000Z`, bucket `examplebucket`, virtual-hosted style.

- **GET Object** (`GET /test.txt`, `Range: bytes=0-9`, signature
  `f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41`) is reproduced **through the
  SDK's own declared `GetObject`** with a frozen clock
  (`official_get_object_example_is_reproduced_by_the_sdk`).
- **PUT Object**, **GET Bucket Lifecycle** and **Get Bucket (List Objects)** are reproduced by the
  SDK's public signer (`sigv4::sign_request`) on native requests carrying exactly the documented
  headers (`official_put_lifecycle_and_list_examples`): the SDK's `PutObject` always sends a
  `Content-Type` and the documented PUT signs a `Date` header instead, and the other two are not
  operations of this SDK (`?lifecycle`, ListObjects v1).
- For all four, the test-side verifier (`tests/common/mod.rs`, written from the documented
  algorithm, independent of `aws-sigv4` except for the SHA-256/HMAC primitives) recomputes the
  canonical request and the string to sign from the request and must equal the documented text,
  and the documented HMAC chain from the documented secret must give the documented signature.
  The fixtures' own consistency was checked when they were extracted: SHA-256 of each documented
  canonical request equals the hash printed in the documented string to sign, and SHA-256 of
  `Welcome to Amazon S3.` equals the documented PUT payload hash.

No other credential-shaped value is used. All other tests sign with synthetic values that are
deliberately not shaped like AWS keys (`SYNTHETIC-ACCESS-KEY-A`, `synthetic-secret-a-not-a-real-key`,
`synthetic-session-token-not-real`, ...).

## Fixtures

`fixtures/**` is marked `-text` in `.gitattributes`, so every byte (line endings included) is
checked out as committed.

| Fixture | Source | Transformations | Status |
|---|---|---|---|
| `sigv4/get-object.{creq,sts,auth}` | sig-v4-header-based-auth, "Example: GET Object": CanonicalRequest, StringToSign, Authorization header | extracted programmatically from the `<pre>` blocks; trailing spaces the HTML adds after some lines removed; no trailing newline | documented example |
| `sigv4/put-object.{creq,sts,auth}`, `sigv4/put-object.payload` | same page, "Example: PUT Object"; the payload is the documented string `Welcome to Amazon S3.` | as above | documented example |
| `sigv4/get-bucket-lifecycle.{creq,sts,auth}` | same page, "Example: GET Bucket Lifecycle" | as above | documented example |
| `sigv4/list-objects.{creq,sts,auth}` | same page, "Example: Get Bucket (List Objects)" | as above | documented example |
| `list-start-after.xml` | API_ListObjectsV2, "Listing keys using the max-keys, prefix, and start-after parameters", sample response | HTTP status line and headers removed (served by the test with `Content-Type: application/xml`); trailing whitespace removed | documented example |
| `list-delimiter.xml` | API_ListObjectsV2, "Listing keys by using the prefix and delimiter parameters", first sample response | leading newline and trailing whitespace removed | documented example |
| `error-no-such-key.xml` | ErrorResponses, "REST error responses" sample | none (the space after `</Resource>` is kept) | documented example |
| `get-object-range.head` | API_GetObject, "Range header" sample response | indentation and status line removed; `Content-Length` and `Connection` removed (the loopback server writes its own framing); the 10 body bytes are synthetic (`0123456789`; the documentation shows `[10 bytes of object data]`) | documented head, synthetic body |
| `head-object.head` | API_HeadObject, first sample response | as above; the test sends 434,234 zero bytes so that `Content-Length` is the documented `434234` | documented head, synthetic body |
| `put-object.head` | API_PutObject, "Versioning enabled" sample response | as above (`Content-Length: 0` written by the server) | documented head |
| `inventory-page-1.xml`, `inventory-page-2.xml` | structure of API_ListObjectsV2 "Using a continuation token" (both responses); the token is the documented `1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=` | hand-authored: keys, sizes, dates and entity tags are synthetic; quotes in `<ETag>` written as `&quot;` (equivalent XML); unknown elements (`FutureListingField`, a root attribute) and documented-but-unmodelled ones (`ChecksumType`, `RestoreStatus`) added; a multipart-style tag (`-2` suffix) and a tag that is not hexadecimal show the SDK treats tags as opaque | synthetic, from documented structure |
| `list-unknown-elements.xml` | elements of the ListObjectsV2 response syntax | hand-authored: namespaced extension, nested unknown elements, unknown attributes, a comment, a processing instruction, repeated `ChecksumAlgorithm`, an unknown storage class, XML entities in a key | synthetic |
| `list-url-encoded.xml` | ListObjectsV2 `encoding-type` / `EncodingType` (the documented `test_file(3).png` → `test_file%283%29.png`) | hand-authored around the documented example key | synthetic, from a documented example |
| `list-truncated-without-token.xml`, `list-not-well-formed.xml` | ListObjectsV2 ("A 200 OK response can contain valid or invalid XML") | hand-authored negative cases | synthetic |
| `error-signature-does-not-match.xml`, `error-precondition-failed.xml` | codes, messages and statuses of the ErrorResponses code table, in the documented `<Error>` structure | hand-authored; an extra unknown element in the first | synthetic |

Other test data is synthetic and written in the tests: object bytes, metadata, the fake's entity
tags (`"opaque-etag-N"`), request ids (`FAKE-REQUEST-ID`), `x-amz-bucket-region`, the 301/307
redirect replies.

## Facts taken from the documentation, and observations

- Requests: the request syntax, URI parameters and headers of the four API pages; the `206` /
  `Content-Range` behaviour and "Amazon S3 doesn't support retrieving multiple ranges of data per
  GET request" (GetObject); `304`/`412` semantics of the four conditional headers (HeadObject,
  GetObject); `If-None-Match: *` and `If-Match` on PutObject (`412`, `409
  ConditionalRequestConflict`); `HEAD` errors carry no body (HeadObject).
- SigV4: `UriEncode` (everything but `A-Z a-z 0-9 - . _ ~`, `%20` not `+`, uppercase hex, `/`
  unencoded inside the object key), no path normalization for S3, sorted canonical query, the
  signed payload option and `x-amz-content-sha256`, "Any x-amz-* headers that you plan to include
  in your request must also be added" to the signed headers.
- **Observed (signer vs documentation):** `aws-sigv4` writes the `Authorization` components
  separated by `, ` (comma, space); the documented header uses `,`. Credential, signed headers and
  signature are identical; the tests compare components and state the separator difference.
- **Observed (documentation inconsistencies):** GetObject's sample `Last-Modified: Wed, 12 Oct 2009
  17:50:00 GMT` names the wrong weekday (12 October 2009 was a Monday), and HeadObject's
  `Last-Modified: Sun, 1 Jan 2006 12:00:00 GMT` has a one-digit day; neither is a valid
  IMF-fixdate. `HttpDate::parse` tolerates both (the date itself is unambiguous); the HEAD fixture
  keeps the one-digit day.
- **Inferred, not documented on the pinned pages:** with `encoding-type=url`, how S3 encodes a
  space. The SDK decodes `+` as a space and `%XX` as a byte; since S3 encodes a literal `+` as
  `%2B` under either convention, this is correct whether S3 writes a space as `+` or `%20`.
- **Not implemented:** the ListObjectsV2/HeadObject note "You must URL encode any signed header
  values that contain spaces" (no header this SDK sends is concerned; the canonicalization rule
  of the signing page — trim, collapse sequential spaces — is what the signer and the verifier
  apply).
- Fixed choices of this SDK, not AWS rules: no dotted bucket names (virtual-hosted style only),
  no period-only key segments, header values restricted to visible ASCII, `max-keys` 1–1000,
  64 KiB error-body limit.

## Third-party crates

| Crate | Version | Why |
|---|---|---|
| `aws-sigv4` | 1.6.4 (`sign-http`, `http1`; default features off) | AWS's own Rust SigV4 implementation (smithy-rs), the signer of the AWS SDK for Rust, with S3-specific settings (single encoding, no path normalization, `x-amz-content-sha256`). Chosen over a hand-written signer or a smaller third-party crate because signature compatibility with S3 is the product, and this is the implementation AWS maintains. It only signs; it executes nothing. Its weight (with `aws-smithy-http`, `aws-smithy-types`, `aws-smithy-async`, `time`, `tracing`, `hmac` 0.13, `sha2` 0.11) is recorded in `FRICTION.md`. |
| `aws-credential-types` | 1.3.1 | the `Credentials` type `aws-sigv4` signs with (converted from this SDK's own `Credentials` at signing). |
| `aws-smithy-runtime-api` | 1.19.2 (`client`) | the `Identity` type of `aws-sigv4`'s signing parameters. |
| `roxmltree` | 0.21.2 | a mature, dependency-free (`memchr` only) read-only XML 1.0 tree: matching by local name makes the namespace optional and unknown elements ignorable without a schema; positions for errors. |
| `httpdate` | 1.0.3 | HTTP date formatting and parsing (IMF-fixdate, RFC 850, asctime), small and widely used (hyper's). |
| `percent-encoding` | workspace (2.3) | `UriEncode` of the query, `encoding-type=url` decoding. |
| dev: `hmac` 0.13.0, `sha2` 0.11.1 | the versions `aws-sigv4` already uses (no duplicate crates) | the test-side verifier's HMAC-SHA256 and SHA-256. |
| dev: `concord-example-testserver`, `tokio` | workspace | the loopback HTTP/1.1 server and the async runtime of the tests. |
