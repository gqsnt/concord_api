# Between waves 2-3

Coordinator integration of the between-waves refinements: a client **send policy** (`send P;`:
every generated `send()` returns `<P as concord::SendWith<T>>::Output`), `Prepared::follow`,
`Received::bytes_limited`, `Received::decode_limited` and `Received::url`. "Before" is
`1f71432c`, "after" the commit that adds this file.

## Verdict

- **Send policy: not adopted, because it cannot be expressed for S3.** A policy is a type whose
  `send` receives only the endpoint; S3's signature needs the client's credentials, Region and
  clock (and the request's credentials override), which neither the endpoint nor the built
  request carries. The forwarding layer stays.
- **`Received::bytes_limited`: adopted** for the `<Error>` document (replaces a hand-written
  bounded chunk loop; same outcomes, now covered by a new boundary test).
- `Prepared::follow`: no use (S3's `301`/`307` name another Region's host and are reported, not
  followed; continuation tokens are query values, not URLs). `Received::decode_limited`: not
  adopted for `ListObjectsV2` pages, since S3 documents no byte size for a page (1,000 keys of up
  to 1,024 bytes, URL-encoded with `encoding-type=url`, plus owners), so any bound would be
  invented. `Received::url`: not needed.

## Why a signing send policy cannot sign S3 requests

What SigV4 needs at send time, today (`src/client.rs`, `Signing::sign`):

```rust
        let (client, mut request, decoder) = prepared.map_err(S3Error::Build)?.into_parts();
        edit(&mut request)?;
        canonicalize_target(request.url_mut());
        let credentials = self
            .credentials
            .as_ref()
            .unwrap_or(&self.context.credentials);
        sign_request(
            &mut request,
            credentials,
            &self.context.region,
            self.context.clock.now(),
        )?;
```

`context` is the client's (`S3Client::new(http, region, credentials)`, `with_credentials`,
`with_clock`); `self.credentials` is the per-request override (`.credentials(c)` on every
operation). A send policy reaches none of it:

- `SendWith::send` is an associated function without a receiver:
  `fn send<E>(endpoint: E) -> impl Future<Output = Self::Output> + Send where E: Endpoint<T> + Send`
  (`crates/concord-core/src/prepared.rs:227-234`);
- the generated `send()` passes only the endpoint: `<#policy as #k::SendWith<#out>>::send(self)`
  (`crates/concord-macros/src/emit.rs:1446`);
- `Endpoint<T>` exposes `prepare()` and `operation()` only (`prepared.rs:187-196`), and the built
  request holds only what the declaration placed on the wire;
- a client value cannot be declared without a placement, so the context cannot ride on the
  client either (`crates/concord-macros/src/model.rs:568`).

Reproduction (observed): a temporary `src/probe.rs`, added with `mod probe;` in `src/lib.rs`,
checked with `cargo check -p concord-sdk-amazon-s3`, then removed:

```rust
pub struct Signed;

impl<T: Send> SendWith<T> for Signed {
    type Output = Result<T, concord::Error>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        // 1. The endpoint is the policy's only input: is the client's context reachable from it?
        let signer: &Signer = endpoint.client();
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        sign_request(&mut request, &signer.credentials, &signer.region, signer.now()).unwrap();
        decoder.execute(&client, request).await?.decode().await
    }
}

#[derive(Clone)]
pub struct Signer {
    credentials: Credentials,
    region: Region,
}

concord::api! {
    // 2. The context as a client value the policy would read, without a wire placement.
    pub client Probe(signer: Arc<Signer>) {
        base "https://s3.us-east-1.amazonaws.com";
        send Signed;
        GET op();
    }
}
```

```text
error: value `signer` is never used; place it (`query`, `header("..")`) or reference it from `path`, `subdomain`, `query` or `headers`
  --> examples/amazon-s3-sdk/src/probe.rs:37:22
   |
37 |     pub client Probe(signer: Arc<Signer>) {
   |                      ^^^^^^

error[E0599]: no method named `client` found for type parameter `E` in the current scope
  --> examples/amazon-s3-sdk/src/probe.rs:16:40
   |
14 |     async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
   |                   - method `client` not found for this type parameter
15 |         // 1. The endpoint is the policy's only input: is the client's context reachable from it?
16 |         let signer: &Signer = endpoint.client();
   |                                        ^^^^^^ method not found in `E`
```

The phase-A example `checks/phase-a/tests/send_policy.rs` (`Signed`) signs with a digest of the
request alone; a real signer's key material is per client.

Ways around it, considered and rejected:

1. **The secret on the request, stripped by the policy** (a `#[sensitive]` client header): the
   generated `prepare()` stays public and is not the policy's, so `prepare()?.send()` or
   `send_raw()` would put the secret access key on the wire (to S3, a proxy, an S3-compatible
   test server). Rejected.
2. **A process-global, thread-local or task-local context**: wrong as soon as two clients hold
   different credentials, and it cannot follow `with_credentials`, `with_clock` or the
   per-request override. Rejected.
3. **A policy that refuses to send, plus `S3Client::send(op)` signing with the client's
   context** (compilable today): the generated builders would be public and the forwarders would
   go, but the caller's call turns inside out (`s3.send(bucket.get_object(key).set_range(r))`),
   the generated `send()` becomes a run-time refusal, and `prepare()` would return an unsigned
   request where today's returns a signed one (doctest in `src/client.rs`, "`authorization`
   ... `is_sensitive()`"). Not clearer for callers; not adopted. It would be an owner's design
   decision, recorded here as an option only.

Even with the context available, three pieces have no place in a policy: user metadata
(`x-amz-meta-{name}`: names are data, not declarable; `FRICTION.md` 4), the per-request
`credentials` override (an input of one call, not on the wire), and a signed `prepare()`. What
would fit: target canonicalization, signing, the status mapping (`finish`), and the `GetObject`
range check, whose requested range is readable from the built request's `range` header (the
builder's copy of it would go).

## Measurements (before = after, except the error reader)

`src/client.rs` at `1f71432c`, by block ("code" excludes blank and comment lines):

| Lines | total | code | Under a context-carrying policy (inferred, not compiled) |
|---|---|---|---|
| 1-46 imports, `http_client_builder` | 46 | 28 | stays |
| 47-219 `Context`, `S3Client` | 173 | 110 | stays (validated endpoint, credentials, clock, `sign`) |
| 220-309 `Bucket` handle | 90 | 70 | goes (generated scope) |
| 310-344 `Signing::sign` | 35 | 32 | moves into the policy |
| 345-404 `SignedRequest<T>` | 60 | 43 | goes (no signed `prepare()` any more) |
| 405-434 `operation_name`, `finish` | 30 | 25 | moves into the policy |
| 435-497 `SignedRequest<T>::send` ×4 | 63 | 35 | goes; the range check moves into the policy |
| 498-608 `ListObjectsV2` type, `ListPage`, setters, `prepare`/`send` | 111 | 72 | goes, except `ListPage` |
| 609-662 `ListObjectsV2::pages` | 54 | 44 | stays, as an inherent method of the generated type |
| 663-671 `Debug` | 9 | 7 | goes |
| 672-730 `read_conditions!` | 59 | 44 | goes |
| 731-776 `HeadObject` | 46 | 28 | goes |
| 777-875 `GetObject` | 99 | 69 | goes |
| 876-1001 `PutObject` | 126 | 88 | goes, except the metadata edit (no carrier) |
| **220-1001 operation layer** | **782** | **557** | |

Forwarding setters in that layer (measured): **36 definitions, 245 lines with their docs (206
code)**, 42 public methods once `read_conditions!` is expanded on `HeadObject` and `GetObject`
(`maybe_token` is private). 32 of the 36 only hand a value to the generated request or scope
(`set_*`/`maybe_*`); the other four (`credentials` ×3, `metadata`) have no declared counterpart.

Error reader (`ApiError::read`, `src/error.rs`): 49 → 42 lines; `BytesMut` and `reqwest`
imports dropped.

## What adding one header parameter costs

Example: `x-amz-checksum-mode` on `GetObject`.

| Before (kept) | With a signing send policy (not expressible today) |
|---|---|
| `src/api.rs`: the value and its placement (`header("x-amz-checksum-mode") checksum_mode?: String,` and a doc line) | the same |
| `src/client.rs`: a forwarding setter on `GetObject` (7 lines with its doc; in `read_conditions!` if `HEAD` shares it) | — (the generated `set_checksum_mode`/`maybe_checksum_mode` are the caller's setters) |
| signing: nothing (every header present is signed, `src/sigv4.rs` `sign_request`) | nothing |

Observed: with the parameter added to the declaration and no forwarder,
`cargo clippy -p concord-sdk-amazon-s3 --all-targets -- -D warnings` passed without a warning
(probe reverted), so a forgotten forwarder is silent: the value is simply unreachable for
callers.

## Adding an operation (say `DeleteObject`)

| Before (kept) | With a signing send policy (not expressible today) |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| 2. a builder type holding the generated request and `Signing`, one forwarding setter per value, `credentials`, `prepare()`/`send()`, `Debug` | — |
| 3. `impl SignedRequest<DeleteObjectOutput> { send }` | its output in the policy's per-output check, if it has one |
| 4. `Bucket::delete_object`, re-exports | — |
| 5. README, tests | 2. README, tests |

## Declaration, adapter and one caller call

Declaration: unchanged (private, no `send` statement); `src/api.rs`'s module doc now says why a
send policy does not change that.

Adapter: unchanged except the error reader. Before (`src/error.rs`, after `into_response()` in
`finish`):

```rust
        let mut buffer = BytesMut::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    if buffer.len() + chunk.len() > MAX_ERROR_BODY {
                        error.body = ErrorBody::TooLarge;
                        return error;
                    }
                    buffer.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(_) => {
                    error.body = ErrorBody::ReadFailed;
                    return error;
                }
            }
        }
```

After (`ApiError::read` takes the `Received<T>`; the body is read inside Concord's boundary):

```rust
        let buffer = match received.bytes_limited(MAX_ERROR_BODY).await {
            Ok(bytes) => bytes,
            Err(e) => {
                error.body = match e.kind() {
                    ErrorKind::Limit => ErrorBody::TooLarge,
                    _ => ErrorBody::ReadFailed,
                };
                return error;
            }
        };
```

One caller call, unchanged (`src/lib.rs` example):

```rust
let first_five = bucket.get_object(key).range(ByteRange::inclusive(0, 4)?).send().await?;
```

With a signing policy it would have been
`bucket.get_object(key).set_range(ByteRange::inclusive(0, 4)?).send().await?`; with option 3
above, `s3.send(bucket.get_object(key).set_range(ByteRange::inclusive(0, 4)?)).await?`.

## What stays correct, and tests

SigV4 (AWS's four example vectors and the independent verifier in `tests/common/mod.rs`), the
key `%2F` → `/` canonicalization, query re-encoding, conditional and range semantics,
`ObjectBody::completion()` and the typed `S3Error` are untouched.

New test `tests/protocol.rs` `error_documents_are_read_up_to_the_bound`: a documented `<Error>`
padded to exactly `MAX_ERROR_BODY` bytes is `Parsed` (`NoSuchKey`), one byte more is
`TooLarge`, an HTML body is `Unrecognized`, a body cut after a prefix is `ReadFailed`. It passes
against both the old chunk loop and `bytes_limited` (checked by restoring the old `error.rs` and
`client.rs`), and fails with the limit lowered by one byte (`left: (TooLarge, None)`, mutation
reverted). No existing test or expectation changed.

Results: unit 9, `first_use` 13, `protocol` 29 (28 + the new one), `workflows` 4, doctests 5 —
all pass; `cargo clippy --all-targets -D warnings` and `cargo doc` (`-D warnings`) clean.

# Between waves 2-3 (after the value-form policy)

Re-evaluation after the library fix that the section above asked for: `SendWith::send` takes
`&self`, and `send signer;` names a required client value used as the policy (not on the wire,
cloned per `send()`, withheld by the generated `Debug`). "Before" is `634b135b`, "after" the
commit that adds this section.

## Verdict

**Adopted.** The generated bucket handle and operation builders are now the public ones
(re-exported as `Bucket`, `ListObjectsV2`, `HeadObject`, `GetObject`, `PutObject`). The client
value `signer: Signer` (Region, credentials and clock behind one `Arc`) is the send policy
(`send signer;`), so every generated `send()` canonicalizes the target, signs, executes once and
maps the response to `S3Error`.

Decisive reason: all 36 forwarding definitions go (245 lines with their docs). A declared value is
now the caller's setter with no other edit, so a forgotten forwarder can no longer happen. A
forgotten policy impl for a new output is a compile error. Every S3 rule keeps its test, and the
two inputs the policy cannot see have a domain replacement.

How the three known non-expressible parts are handled:

| Part | Handling | Cost |
|---|---|---|
| User metadata (`x-amz-meta-{name}`, names are data) | **A remaining domain method.** `PutObject::metadata(m)` returns `PutObjectWithMetadata`, which has `send(&s3)` and `prepare(&s3)`. The headers are added before signing by the client's `Signer` (the same `sign` path). | It must be the last call of a put, and it needs the client in scope (first_use's `bucket()` helper had to be split into `client()`/`bucket()`). It signs with the client passed in: pass the one the request came from (documented). |
| Per-request credentials override (`.credentials(c)` ×3) | **Removed.** Use a client snapshot instead: `s3.clone().with_credentials(c).bucket(..)` (existing API, same immutable-snapshot semantics, one `Arc` per call). | Callers who changed the credentials of a builder they already held must now create it from the derived client. |
| `prepare()` signed | **Moved, and documented.** The generated `prepare()` is the declared request, **unsigned** (Concord's §5.13 contract). `S3Client::prepare(op)` (generic over `Endpoint<T>`) returns the same `SignedRequest<T>` as before. | The change is silent for callers (measured below). |

Also given up: `Bucket::name()` (generated scopes have no getters), and the domain spellings
`range()`, `if_absent()`, `requester_pays()`, `url_encoded_keys()`. These are now `set_range`,
`set_if_none_match(AnyObject)`, `set_requester_pays(Requester)` and
`set_encoding_type(UrlEncoding)`, and the three marker types are public. The put body is now a
`Bytes` (re-exported; `IntoValue` is sealed, so `&str`/`Vec<u8>` are no longer accepted). Kept by
changing the declaration: `content_type: ContentType` (sent through a `CONTENT_TYPE` template)
and `response_content_type?: ContentType`, so both stay validated. Gained: `maybe_*` on every
optional value, `set_key`/`set_data`, and generated per-value docs (moved from the forwarders onto
the declared values).

Alternatives rejected for metadata:

- **A pseudo-header that the policy expands.** `prepare()` and the generated docs would show a
  header S3 does not have.
- **A task-local.** It needs a runtime dependency and is hidden coupling.
- **Keeping a domain `PutObject` builder.** It needs a domain `Bucket` holding the signer, plus 8
  forwarders back.

## How the policy signs

`src/client.rs` (after):

```rust
macro_rules! signed_send {
    ($($output:ty),* $(,)?) => {$(
        impl SendWith<$output> for Signer {
            type Output = Result<$output, S3Error>;

            async fn send<E: Endpoint<$output> + Send>(&self, endpoint: E) -> Self::Output {
                self.sign(endpoint.prepare(), |_| Ok(()))?.send().await
            }
        }
    )*};
}

signed_send!(
    ListObjectsV2Output,
    ObjectHead,
    GetObjectOutput,
    PutObjectOutput
);
```

`Signer::sign` is the former `Signing::sign`: `into_parts` → edit (metadata only) →
`canonicalize_target` → `sign_request`. The per-output checks are the unchanged
`SignedRequest<T>::send` impls.

The builders' private copies are replaced by read-backs from the built request, each the exact
inverse of what the declaration wrote:

- the `GetObject` range is read from the `range` header (`ByteRange::from_header`, the inverse of
  `Display`, with a new unit test);
- the starting token of `pages()` is read from the `continuation-token` query pair.

Both are covered by existing tests. Mutations (each reverted):

- dropping the range read-back fails `refused_ranges_and_body_framing` and
  `a_range_mismatch_is_rejected_and_200_fallback_is_explicit`;
- starting every chain at `Resume::Start` fails `a_failed_page_retains_items_and_can_resume`,
  `a_broken_continuation_contract_is_an_error`, `a_page_cap_is_distinct_from_exhaustion`,
  `a_page_cap_stops_with_the_token_to_resume_from` and
  `workflow_a_resumes_an_interrupted_inventory_from_its_token`.

## Measurements

`src/client.rs` ("code" excludes blank and comment lines):

| Block | before total/code | after total/code |
|---|---|---|
| imports, `http_client_builder` | 46 / 28 | 40 / 20 |
| `Context`, `S3Client` (after: + `prepare(op)`, `with_context`) | 173 / 110 | 192 / 119 |
| `Bucket` handle | 90 / 70 | — (generated) |
| `Signing::sign` → `Signer` (`sign`, `Debug`) + `signed_send!` | 35 / 32 | 83 / 66 |
| `SignedRequest`, `operation_name`, `finish` | 89 / 67 | 89 / 67 |
| `SignedRequest<T>::send` ×4 | 63 / 35 | 63 / 35 |
| `ListObjectsV2` type, setters, `prepare`/`send`, `Debug` | 119 / 79 | — (generated) |
| `ListPage`, `ObjectPages`, `pages` | in the row above + 55 / 44 | 81 / 60 |
| `read_conditions!`, `HeadObject`, `GetObject` | 204 / 141 | — (generated) |
| `PutObject` (after: `metadata`, `PutObjectWithMetadata`) | 127 / 88 | 67 / 40 |
| **file** | **1000 / 694** | **614 / 407** |

- **Forwarding definitions:** 36 → **0**. The callers' builder surface went from 42 hand-written
  public methods to 82 generated `set_*`/`maybe_*` controls (counted in rustdoc), plus `pages`,
  `metadata`, `prepare` and `send`.
- **`src/api.rs`:** 137 / 108 → 195 / 113. That is +53 doc lines moved from the forwarders onto
  the declared values, and +5 code lines: the `signer` value, `send signer;` and the content-type
  template.
- **`src/lib.rs`:** 90 / 28 → 101 / 34 (re-exports).
- **`src/types.rs`:** +45 lines (the range read-back with its unit test, and docs on the public
  markers).
- **Hand-written `client.rs` + `api.rs` + `lib.rs`:** 1227 / 830 → 910 / 554.

## What adding one header parameter costs

| Before | After |
|---|---|
| `src/api.rs`: the value and its doc line | the same (2 lines) |
| `src/client.rs`: a forwarding setter (7 lines with its doc), silent if forgotten (observed in the section above) | nothing; there is no forwarder to forget |

Observed with a probe, then reverted. I added
`header("x-amz-checksum-mode") checksum_mode?: String` to `get_object` and a temporary test
calling `.set_checksum_mode("ENABLED").send()`. The test passed: the header reached the wire and
is in `SignedHeaders` (independent verifier), and `cargo clippy --all-targets -D warnings` was
clean.

Adding an operation now takes the `api!` entry and, if its output type is new, one line in
`signed_send!` plus its `SignedRequest<T>::send`. Forgetting them is a compile error. Observed
with a probe `DELETE delete_object(key: ObjectKey)`, reverted:
``error[E0277]: the send policy `Signer` does not send a `()` output``, pointing at
`signer: Signer`.

## Caller excerpt

Before (`src/lib.rs` example):

```rust
bucket
    .put_object(key.clone(), "hello, world", ContentType::new("text/plain")?)
    .metadata(metadata)
    .send()
    .await?;
let first_five = bucket.get_object(key).range(ByteRange::inclusive(0, 4)?).send().await?;
```

After:

```rust
bucket
    .put_object(key.clone(), Bytes::from_static(b"hello, world"), ContentType::new("text/plain")?)
    .metadata(metadata)
    .send(&s3)
    .await?;
let first_five = bucket.get_object(key).set_range(ByteRange::inclusive(0, 4)?).send().await?;
```

| Use | Before | After |
|---|---|---|
| Per-request credentials | `bucket.list_objects_v2().credentials(c).send()` | `s3.clone().with_credentials(c).bucket(name).list_objects_v2().send()` |
| Signed inspection | `bucket.get_object(k).prepare()?` | `s3.prepare(bucket.get_object(k))?` |

Migrating this crate's own tests (observed):

- **Renamed setters:** 67 calls. rustc's E0599 suggested each `set_*`, for example "there is a
  method `set_prefix` with a similar name".
- **Other edits:** 9 bodies became `Bytes`; 3 metadata puts were reordered and now call
  `send(&s3)`; 1 per-request credentials call now uses a client snapshot; 12 signed `prepare()`
  calls now go through `s3.prepare(op)`.
- **No compile error for `prepare()`:** before that last step, the 12 `prepare()` sites compiled
  unchanged against the unsigned generated `prepare()`, which has the same `request()` and
  `into_parts()`. All 4 tests containing them failed at run time (no `authorization`, a `%2F`
  target, a `403`). A downstream caller that inspects or executes `prepare()` gets no compile
  error.
- **No expectation changed:** no assertion or expected value was touched. Every removed test line
  is a call site.

## Library gaps (with reproductions)

1. **A per-request input to the send policy cannot be declared.** Adding
   `metadata?: crate::types::Metadata,` to `put_object` (no placement) fails with
   ``error: value `metadata` is never used; place it (`query`, `header("..")`) or reference it
   from `path`, `subdomain`, `query` or `headers` `` at `src/api.rs:181`. The policy sees only the built
   request, so per-request metadata (and per-request credentials) stay outside the generated
   `send()`.
2. **A decorated endpoint cannot go through the policy.** `Endpoint::prepare` must return a
   `Prepared<T>`, and a `Prepared` cannot be edited or rebuilt after `into_parts()`. Its public
   methods are `follow`, `operation`, `request`, `send`, `receive`, `send_raw` and `into_parts`
   (`crates/concord-core/src/prepared.rs`). So `PutObjectWithMetadata` signs through the client
   (`Signer::sign` with an edit) rather than implementing `Endpoint`.
3. **Generated handles have no read access to their values.** Hence the two read-backs above
   (exact and tested) and the loss of `Bucket::name()`.
4. **Not a defect: `prepare()` changes meaning silently when a policy is added.** This is
   documented Concord behavior and a migration hazard for an SDK's callers (measured above).

The generated `Debug` labels use the generated names (`S3ApiBucketGetObject { .. }`). A
`content_type` placed through the native `CONTENT_TYPE` key prints `<redacted>`.

## What stays correct, and tests

These are covered by the same tests:

- SigV4 against AWS's four example vectors. The GET vector is reproduced through
  `S3Client::prepare` on the generated `GetObject`.
- The independent verifier, which checks every request the fake receives.
- The key `%2F` → `/` canonicalization and query re-encoding (`canonicalize_target`, unchanged,
  now called in `Signer::sign`).
- Conditional and range semantics, `ObjectBody::completion()`, the typed `S3Error`, and
  `bytes_limited` error documents.

New tests:

- `types::tests::ranges_read_back_from_their_header`;
- `tests/protocol.rs` `generated_builders_withhold_the_signing_context`: the `Debug` output of
  `Bucket`, `ListObjectsV2`, `GetObject`, `PutObjectWithMetadata` and `SignedRequest` contains
  neither secret, nor the session token, nor `Signature=`, and the generated `prepare()` carries
  no `authorization`.

Results: all pass.

- unit: 10 (9 + 1);
- `first_use`: 13;
- `protocol`: 30 (29 + 1);
- `workflows`: 4;
- doctests: 5.

`cargo clippy --all-targets -- -D warnings`, `cargo doc` (`-D warnings`) and `cargo fmt --check`
are clean.
