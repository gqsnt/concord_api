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
