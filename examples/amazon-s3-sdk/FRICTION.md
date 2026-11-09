# Friction log: the Amazon S3 SDK on Concord 4.2.0 (candidate `94d13527`)

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Written while implementing `examples/amazon-s3-sdk` from scratch against the public API of
`concord` (DSL, `Prepared`, `Decoder`, `Received`, `Encode`/`Decode`/`Incremental`, `Operation`)
and `concord-pagination` (`Pages`). Nothing in `crates/`, `docs/`, `scripts/`, `checks/` or other
examples was edited. Reading material: `README.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CALLER_GUIDE.md`, `docs/RECIPES.md`, `docs/CUSTOM_FORMATS.md` (first sections),
`crates/concord-pagination/README.md`, the `concord-core` sources of `Prepared`, `Decoder`,
`Received`, `Incoming`, `ResponseHead`, `CodecError` and the codec traits, and from the existing
SDKs `examples/salesforce-sdk/src/transport.rs`, `examples/kubernetes-sdk/src/{lib,context,api}.rs`
and the first 200 lines of `examples/github-sdk/src/download.rs` (orientation only; none of them
supplied a solution to the signing, encoding or HEAD problems below).

Each entry: what happened, the reproduction (`file:line` in this crate), the cause (**SDK**,
**provider**, **application**, **docs**, **Concord**, **test support**) and an assessment.
Entries 1–9 are the things that had to be worked around, most significant first; 10–17 what
Concord made easy.

## Worked around

### 1. Every request must be signed after `prepare()`: the declaration is private and every operation is re-wrapped (Concord)

- **Contract.** SigV4 signs the final request: method, target, the headers present, the payload
  hash. It can only be computed after Concord has built the request, and must be added before it
  is executed.
- **Path taken.** The documented native route, for every call:
  `prepare()?.into_parts()` → edit → sign → `Decoder::execute(&client, request)` → `Received` →
  the SDK's status mapping (`src/client.rs:318-343` `Signing::sign`, `src/client.rs:376-384`,
  `src/client.rs:412-433` `finish`). This works and keeps the endpoint's identity and decoder.
- **The cost is the generated surface.** The generated `send()` would send the request unsigned
  (S3 answers `403`), so the generated client cannot be handed to callers: `src/api.rs` is a
  private module, and each operation is an SDK builder holding the generated request and
  forwarding its setters one by one — 37 forwarding methods (`src/client.rs:501-1001`, e.g.
  `ListObjectsV2::prefix` at `src/client.rs:524`, the `read_conditions!` macro at
  `src/client.rs:673` used by two builders). That is reconstructing the generated builder API by
  hand, and every value added to the declaration needs a forwarder (the compiler does not point
  at a missing one).
- **Also lost.** Concord's redacted `Debug` of `Prepared` does not survive `into_parts`; the SDK's
  `SignedRequest` writes its own (`src/client.rs:387-403`). The `#[sensitive]` on
  `continuation-token` (`src/api.rs:47`) only affects the generated request's `Debug` inside the
  SDK builders.
- **Reproduction.** Remove the wrappers and expose `S3ApiBucketGetObject`: `.send()` compiles and
  sends no `Authorization` header.
- **Assessment.** The first provider in this campaign whose authentication depends on the built
  request. A per-client or per-scope request hook between building and execution (`FnOnce(&mut
  reqwest::Request)`, run by `Prepared::send`/`receive`) would let the generated operations stay
  public with their setters, `prepare()`, `send()` and `receive()`. A proposal only.

### 2. An object key spans path segments: Concord writes `/` as `%2F`, S3 signs and expects `/` (Concord)

- **Contract.** SigV4 for S3: "Encode the forward slash character, '/', everywhere except in the
  object key name" (`photos/Jan/sample.jpg` is the path `/photos/Jan/sample.jpg`), and the path
  is signed as sent, without normalization.
- **Concord.** A path value is exactly one segment (`a/b` → `a%2Fb`); there is no multi-segment
  path value (documented, `docs/RECIPES.md` "A resource key spanning path segments").
- **Path taken.** After `into_parts`, `%2F` in the path is turned back into `/`
  (`src/sigv4.rs:202-206`). This is exact only because Concord's path encoding is "everything but
  unreserved" (a literal `%` becomes `%25`, so `%2F` can only come from `/`) and the endpoint is
  required to have no path of its own (`src/client.rs:118-149`). The recipe's
  `path_segments_mut().extend(..)` was not used: `url` leaves sub-delimiters such as `+`, `=`, `!`
  unencoded in a segment, which is not SigV4's `UriEncode`.
- **Consequence.** `url::Url::set_path` removes `.` and `..` segments, so keys containing them
  are refused (`src/types.rs`, `ObjectKey::new`); AWS recommends against them anyway.
- **Assessment.** Works and is tested (`perturbing_the_key_changes_only_its_own_request`), but the
  SDK depends on an undocumented detail (the exact encode set of path values). A "greedy" path
  value (one value, `/` kept, everything else encoded like today) would remove the rewrite.

### 3. Query values are form-encoded (`+` for a space); SigV4 requires `UriEncode` (`%20`) (Concord, provider)

- **Contract.** SigV4: "The space character ... must be encoded as "%20" (and not as "+")";
  query parameters are `UriEncode`d.
- **Concord.** Query values go through `url::form_urlencoded` (`prefix=a+b`, `*` raw).
- **Path taken.** `canonicalize_target` decodes the pairs and re-encodes them with `UriEncode`,
  in order (`src/sigv4.rs:207-228`), so what is sent is what is signed. Without it, `aws-sigv4`
  canonicalizes `+` as `%20` while S3 may read a raw `+` differently; the test
  `query_values_are_uri_encoded_once_and_signed` fails when the rewrite is removed (checked by
  mutation during development).
- **Assessment.** Small and exact (form decoding inverts form encoding), but it is a second
  native rewrite of what Concord built. An encoding choice for query values (form vs RFC 3986)
  would cover it.

### 4. User metadata header names are data (`x-amz-meta-{name}`) (Concord, by design)

- Header keys must be literals or `HeaderName` constants, so a map of metadata cannot be
  declared. The SDK appends the headers to the native request before signing
  (`src/client.rs:968-982`). Expected for dynamic names; it adds to entry 1's native step.

### 5. Header values of domain types need templates, repeated per endpoint (Concord, minor)

- `ToHeaderValue` is a closed set, so `AccountId`, `ETag`, `ByteRange`, `HttpDate` and
  `StorageClass` go through `headers { "x" = "{value}"; }` with an optional value
  (`src/api.rs:40-43`, `src/api.rs:71-77`, `src/api.rs:99-105`, `src/api.rs:118-129`). The compile
  error for the first attempt (`header("x-amz-expected-bucket-owner") expected_owner?: AccountId`)
  was precise and named the fix. The five conditional/range headers are written twice (HEAD and
  GET): sibling endpoints cannot share an endpoint-level parameter group without moving the values
  to a scope handle, which would change their meaning (per request → per handle).

### 6. A format sees the response, not the request: range validation lives in the SDK wrapper (Concord, minor)

- Whether a `206` answers the requested range needs the request's range. `Incremental::context`
  sees only the head, so the builder keeps a copy of the range next to the generated request
  (`src/client.rs:793`, `src/client.rs:853`) and checks after decoding (`src/client.rs:469-483`).
  The checks that need only the head (`multipart/byteranges`, a `206` without `Content-Range`,
  `Content-Length` vs `Content-Range`) stay in the format (`src/object.rs:162-184`).

### 7. Error documents: the bounded read is the SDK's again (application/SDK, minor)

- A non-2xx keeps the unread response (good); reading it under a limit and parsing S3's
  `<Error>` is SDK code (`src/error.rs:264-311`), the same shape as in the other provider SDKs.
  `HEAD` errors have no body (documented by AWS), which the SDK reports as `ErrorBody::Empty`.

### 8. The loopback test server cannot send a `HEAD` response with a non-zero `Content-Length` (test support)

- `examples/testserver` always derives `content-length` from the body it writes; a `HEAD` reply
  must declare the length without sending the bytes. The fake sends the bytes with
  `connection: close` so the client discards them with the connection
  (`tests/common/mod.rs:682-691`). A `Body::Declared(len)` (head only) would be the clean form.
  Virtual-hosted addressing on loopback also needs `ClientBuilder::resolve` per bucket host
  (`tests/common/mod.rs:67-81`), as `docs/QUICKSTART.md` §6 says.

### 9. The signer: weight, separators, no canonical request (SDK choice, provider docs)

- `aws-sigv4` 1.6.4 is AWS's own signer and reproduced all four documented signatures, but it
  added 30 packages to `Cargo.lock` (`aws-smithy-{http,types,async,runtime-api}`, `time`, a second
  `hmac`/`sha2`/`digest` generation, `base64-simd`, ...), and its API needs
  `aws-credential-types` and `aws-smithy-runtime-api` types (`src/sigv4.rs:285-300`).
- It writes `Authorization` components separated by `, `; AWS's documented headers use `,`
  (`tests/protocol.rs:73-81` compares components and states the difference). It does not expose
  the canonical request or string to sign, so the tests carry an independent verifier
  (`tests/common/mod.rs:237-351`), which is also what lets the fake S3 refuse a mis-signed
  request.
- AWS's own examples contain two invalid HTTP dates (a wrong weekday in GetObject, a one-digit day
  in HeadObject); `HttpDate::parse` tolerates both (`src/types.rs:497-540`). Provider cause.

## Made easy

10. **Virtual-hosted addressing is two statements.** `subdomain "{bucket}"` on the bucket scope
    and `with_base_url` for the endpoint (`src/api.rs:39`, `src/client.rs:143`); invalid labels
    are Concord build errors, so the SDK's bucket-name check only adds S3's own rules.
11. **Response formats that read the head.** `HeadObject` and `PutObject` results are parsed
    entirely in `Decode::context` (`src/object.rs:138-148`, `src/object.rs:356-382`): a malformed
    header is a `Metadata` error with the response kept unread, at no extra code.
12. **`-> stream` with a format of mine.** `GetObject` declares `-> stream Bytes` with
    `response Download;` (`src/api.rs:97-106`); `Incremental::context` validates the head,
    `open` hands back the parsed head with the unread body (`src/object.rs:158-199`). Callers get
    a typed head plus a cancellation-safe body without a native `send_raw`.
13. **The XML listing format** (`src/xml.rs:28-66`): `check_content_type` refuses a non-XML 2xx
    before the body is read, `CodecError::at("line"/"column")` carries positions, and the body
    decoder is ordinary code over `roxmltree`.
14. **Binary body with an explicit content type.** `body data: Bytes` + `request Binary;` +
    `header(CONTENT_TYPE) content_type: String` (`src/api.rs:110-131`): the header replaces
    Binary's `application/octet-stream`, exactly as the author guide's table says, and the body
    stays in memory (`reqwest::Body::as_bytes`) for the payload hash.
15. **Operation identity for free.** Every error names the declared endpoint through
    `Decoder::operation()` / `Received::operation()` (`src/client.rs:405-407`,
    `src/client.rs:629`): no operation name is written by hand. (The `Option` needs a fallback
    that never occurs for generated endpoints.)
16. **`Decoder::execute` + `Received`.** After a native edit, the response still gets the
    endpoint's handling, `head()` before the body, and `into_response()` for the error path
    (`src/client.rs:376-433`). No reimplementation of status checks or transport-error redaction.
17. **`concord_pagination::Pages`** with page metadata (`Page { items, next, meta }`) fit S3's
    continuation token directly (`src/client.rs:619-661`); caps, `Bounded { resume }`,
    `collect_complete()` and cancellation-safe `next_page` came with it, and the SDK only adds
    S3's two contract checks (truncated without token, repeated token).

Header values are validated by `http` (a CRLF in `Cache-Control` is a Concord `Build` error,
`invalid_inputs_send_nothing`), and nothing in the generated code needed a lint allowance.

### Maintenance reproduction: Unicode key and signed range (existing entries 1, 2 and 6)

The maintenance scenario uses the existing public builders and the same native prepare, target
rewrite, signing and decoder execution described above. Run
`CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-amazon-s3 --test workflows changed_key_and_range_are_signed_on_the_wire_without_affecting_other_calls`.
It sends an ASCII key and then a space/Unicode key, changes the signed range separately, and
checks the loopback canonical requests, independently verified signatures, both listing pages and
an unrelated HEAD. The working path needed no additional workaround; see `MAINTENANCE.md` for
which example data and expectations changed. This test does not mutate the SDK to prove the
failure mode when the existing rewrite is removed.

## Coordinator note: after the value-form send policy (between waves 2-3)

This note changes the status of entries 1, 4 and 6; the entries themselves are kept as they were
written.

- **Entry 1 is resolved.** The send policy can now be a client value (`send signer;`), so the
  declaration is public and its generated builders are the SDK's. The 36 forwarders are gone, and
  `Signer::sign` in `src/client.rs` does the native prepare → rewrite → sign path once.
- **Entry 4 stands.** The metadata headers are now added in `PutObjectWithMetadata::prepare`,
  which is sent with the client (`.metadata(m).send(&s3)`).
- **Entry 6 has changed.** The policy reads the requested range back from the built request's
  `range` header; the builder no longer keeps a copy.

Measurements, the remaining limits and library gaps: `INTEGRATION.md`, "Between waves 2-3 (after
the value-form policy)".
