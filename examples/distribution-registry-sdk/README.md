# concord-sdk-distribution-registry

A provider SDK slice for the **CNCF Distribution registry HTTP API v2** (the Docker registry
protocol), written with `concord::api!`: discover repositories and tags, pull a manifest and
verify its blobs, push blobs in resumable chunks and publish a manifest, with the registry's
Bearer token challenges answered. It targets the protocol as the Distribution spec documents it;
that is not a claim that every registry deployment implements every optional feature the same
way.

Sources: the [registry HTTP API V2 spec](https://distribution.github.io/distribution/spec/api/)
and the [token authentication spec](https://distribution.github.io/distribution/spec/auth/token/)
(checked 2026-11-06; see `PROVENANCE.md`).

## What it covers

| Operation | Request | SDK entry point |
|---|---|---|
| API version check | `GET /v2/` | `Registry::ping` |
| Catalog, paged | `GET /v2/_catalog?n=&last=`, then `Link: rel="next"` | `Registry::catalog` |
| Tags, paged | `GET /v2/<name>/tags/list?n=&last=`, then `Link` | `Repository::tags` |
| Manifest pull | `GET /v2/<name>/manifests/<tag or digest>` with an explicit `Accept` | `Repository::manifest` |
| Manifest push | `PUT /v2/<name>/manifests/<reference>` | `Repository::put_manifest` |
| Blob existence | `HEAD /v2/<name>/blobs/<digest>` | `Repository::blob_info` |
| Blob pull, verified | `GET /v2/<name>/blobs/<digest>` | `Repository::blob`, `blob_for` |
| Upload start | `POST /v2/<name>/blobs/uploads/` | `Repository::start_upload` |
| Chunk | `PATCH <Location>` with `Content-Range: <start>-<end>` | `BlobUpload::patch` |
| Upload status | `GET <Location>` | `BlobUpload::status` |
| Completion | `PUT <Location>&digest=<digest>` | `BlobUpload::complete` |
| Whole blob | start, chunks, completion | `Repository::push_blob` |
| Token flow | `401` + `WWW-Authenticate: Bearer` → `GET <realm>?service=&scope=` → retry | automatic, `TokenAuth` |

Also: the documented error document (`{"errors": [{"code", "message", "detail"}]}`, every code of
the spec's table, unknown codes kept as `ErrorCode::Unknown`), `Docker-Content-Digest`,
`Docker-Upload-UUID` and `Docker-Distribution-API-Version`.

Manifest media types: Docker image manifest schema 2
(`application/vnd.docker.distribution.manifest.v2+json`) and the OCI image manifest
(`application/vnd.oci.image.manifest.v1+json`).

**Not covered:** deletion and garbage collection; cross-repository blob mounts; monolithic
`POST ?digest=` uploads (a monolithic upload here is a start plus a completing `PUT` with the
body); manifest lists / image indexes, schema 1 and artifact manifests; foreign-layer `urls` and
blob redirects to storage on other origins; range reads of blobs; credential helpers and
`~/.docker/config.json`; refresh tokens, `offline_token` and OAuth2 `POST` token requests; Basic
authentication of the registry itself (only of the token service); public registry traffic.

## Construction

```rust,no_run
use concord_sdk_distribution_registry::{BasicCredentials, Registry, RepositoryName, TokenAuth};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
// Timeouts, proxies, TLS and redirects belong to your reqwest::Client.
let http = concord::reqwest::Client::builder()
    .timeout(std::time::Duration::from_secs(30))
    .redirect(concord::reqwest::redirect::Policy::none())
    .build()?;
let credentials = BasicCredentials::new("ci-bot", "synthetic-password-for-docs")?;
let registry = Registry::new(http, "https://registry.example.test")?
    .token_auth(TokenAuth::basic(credentials).trust_realm("https://auth.example.test")?);
let repository = registry.repository(&RepositoryName::new("acme/tools/app")?)?;
# let _ = repository; Ok(()) }
```

- `Registry::new` takes the registry's base URL (a path prefix is kept: `https://host/mirror`
  sends `/mirror/v2/...`). Without `token_auth`, a `401` is returned as the registry's answer
  with its parsed challenge (`ApiError::challenge`).
- Disable redirects on the supplied HTTP client as above, or configure a policy that rejects
  cross-origin redirects. The SDK validates returned upload locations and token realms, but the
  supplied client follows redirects before the SDK can inspect the response. A redirect can send
  a request or its URL query to another origin even when reqwest removes `Authorization`.
- `TokenAuth::anonymous()` or `TokenAuth::basic(credentials)`; `trust_realm(url)` trusts that
  realm's **origin**. A challenge naming any other realm is refused before anything is sent to it
  (`AuthProblem::UntrustedRealm`), so credentials never follow a registry's challenge to a host
  you did not name. The registry's own origin is always trusted.
- A repository name is validated by the spec's rule (components `[a-z0-9]+(?:[._-][a-z0-9]+)*`
  joined by `/`, under 256 characters) and sent as several path segments, as written.

## The token flow

On a `401` with `WWW-Authenticate: Bearer realm=..,service=..,scope=..`, the SDK asks the realm
`GET <realm>?service=<service>&scope=<scope>&scope=...` (one `scope` parameter per scope the
challenge lists; the operation's own scope when it lists none) and retries the request **once**
with `Authorization: Bearer <token>`. A second `401` is the registry's answer, never another
round. Tokens (`token`, or the OAuth 2.0 alias `access_token`) are cached per access — pull
(`repository:<name>:pull`), push (`repository:<name>:pull,push`), catalog
(`registry:catalog:*`), none (`GET /v2/`) — and sent pre-emptively until `expires_in` (60 s when
absent) has passed; an `expires_in` too large for the clock is `AuthProblem::TokenResponse`, and
that token is neither cached nor sent. A pull token is never sent with a push request. Tokens, credentials and
upload URLs never appear in this SDK's `Debug` or `Display` output.

## Workflow A: discover, pull, verify

```rust,no_run
use concord_sdk_distribution_registry::{
    ListOptions, ManifestMediaType, PageSize, Reference, Registry, RepositoryName,
};

# async fn run(registry: Registry) -> Result<(), Box<dyn std::error::Error>> {
// The catalog as this registry shows it to this credential (it may be partial or disabled).
let mut catalog = registry.catalog(ListOptions::default().page_size(PageSize::new(100)?));
while let Some(page) = catalog.next_page().await? {
    println!("{} repositories (more: {})", page.items.len(), page.next.is_some());
}

let repository = registry.repository(&RepositoryName::new("acme/app")?)?;
let tags = repository.tags(ListOptions::default()).items().collect_complete().await?;

// An explicit, supported Accept; the bytes are hashed and checked as received.
let manifest = repository
    .manifest(&Reference::tag("v1")?, &[ManifestMediaType::OciV1, ManifestMediaType::DockerV2])
    .await?;
for descriptor in manifest.image.blobs() {
    let info = repository.blob_info(&descriptor.digest).await?; // HEAD: None for 404
    assert_eq!(info.and_then(|i| i.size), Some(descriptor.size));
    // Verified against the descriptor's digest and size; nothing returned unless it matched.
    let bytes = repository.blob_for(descriptor).await?.read_to_end(descriptor.size).await?;
    println!("{} verified ({} bytes)", descriptor.digest, bytes.len());
}
# let _ = tags; Ok(()) }
```

For a large blob, read it chunk by chunk with `BlobReader::next_chunk`: the chunks are
**unverified** until `next_chunk` returns `Ok(None)`, which happens only after the length and the
digest matched. A body longer than expected fails at the first byte too many.

## Workflow B: chunked upload, resume, publish

```rust,no_run
use concord_sdk_distribution_registry::concord::bytes::Bytes;
use concord_sdk_distribution_registry::{
    Descriptor, Digest, ImageManifest, ManifestMediaType, Reference, RegistryError, Repository,
};

# async fn run(repository: Repository, layer: Bytes, config: Bytes) -> Result<(), RegistryError> {
let layer_digest = Digest::sha256_of(&layer);
let mut upload = repository.start_upload().await?;
while (upload.offset() as usize) < layer.len() {
    // Always slice the source at the registry's acknowledged offset.
    let start = upload.offset() as usize;
    let end = layer.len().min(start + 4 * 1024 * 1024);
    if let Err(error) = upload.patch(layer.slice(start..end)).await {
        if !upload.needs_status() {
            return Err(error); // a documented refusal (a 416 already moved the offset)
        }
        // The chunk's fate is unknown (a dropped connection, say): ask the registry.
        upload.status().await?;
    }
}
upload.complete(&layer_digest, None).await?;

let chunk = std::num::NonZeroUsize::new(4 * 1024 * 1024).unwrap();
let config_pushed = repository.push_blob(config.clone(), chunk).await?;
let image = ImageManifest::new(
    ManifestMediaType::OciV1,
    Descriptor::new("application/vnd.oci.image.config.v1+json", config.len() as u64, config_pushed.digest),
    vec![Descriptor::new("application/vnd.oci.image.layer.v1.tar", layer.len() as u64, layer_digest)],
);
let pushed = repository.put_manifest(&Reference::tag("v1")?, &image.to_upload()?).await?;
println!("published {}", pushed.digest);
# Ok(()) }
```

(The loop above resumes without bound; an application caps its own attempts.)

## Upload semantics

- **Server URLs only.** Every chunk, status query and completion goes to the **latest**
  `Location` the registry returned (resolved against the request URL), opaque query state
  included, byte for byte. Completion appends `digest=<digest>` after the registry's own query
  parameters. The SDK never builds an upload URL from `Docker-Upload-UUID`. A `Location` on
  another origin (or with user information) is refused before anything is sent to it.
- **Inclusive ranges.** `Content-Range: <start>-<end>` and `Range: [bytes=]0-<end>` are inclusive.
  `offset()` is the number of bytes the registry acknowledged. A `202` must acknowledge exactly
  the chunk sent. Distribution writes `0-0` for a fresh session *and* for one byte received, so
  the SDK reads `0-0` as zero and refuses a one-byte **first** chunk (`OneByteFirstChunk`); a
  one-byte blob goes with the completing `PUT` (which `push_blob` does).
- **Partial outcomes.** A chunk or completion whose answer was not read as documented (transport
  failure, a protocol violation, a non-2xx other than `416`, a dropped future) leaves
  `needs_status() == true`; `patch` and `complete` then refuse (`StatusRequired`) until `status()`
  has read the registry's authoritative range. A status range beyond the bytes ever sent is a
  protocol error. A `416` carries the last valid range and `Location`: the session adopts them and
  the error is returned, so the caller re-slices from `offset()`. The SDK retries nothing itself
  (apart from the one Bearer challenge answer).
- **Completion.** `201 Created` is required; a `Docker-Content-Digest` with the same algorithm
  must equal the digest given. `400 DIGEST_INVALID` means the uploaded bytes have another digest;
  the spec says such an upload must be restarted.

## Listing semantics

- The first page is the declared request (`n`, `last`); every later page is the
  `Link: <url>; rel="next"` target **as sent** (the registry may change `n`), never a URL the SDK
  builds. A link to another origin or another collection, a link repeating the page just
  requested, or a malformed `Link` header is an error (`PagesError::Provider`), never "last page".
  `Ok(None)` from `next_page()` means a page without `rel="next"` was read.
- The catalog's contents are the registry's choice: it may be complete, limited to what the
  credential may access, missing upstream entries (a mirror), or unavailable (`404`, `401`/`403`,
  `UNSUPPORTED`). A repository missing from it may still exist. Tag lists must name the requested
  repository (`NameMismatch` otherwise); a `null` list reads as empty.

## Verification

- A manifest's digest is computed over the exact bytes received. By digest, the bytes must have
  that digest; by tag, a same-algorithm `Docker-Content-Digest` must name them (another algorithm
  is ignored, as the spec allows). Its `Content-Type` must be one of the accepted types — checked
  before the body is read — and a `mediaType` field must agree with it. At most 4 MiB is read.
- A blob is verified against the digest it was fetched by and, with `blob_for`, the descriptor's
  size; a contradicting `Content-Length` or `Docker-Content-Digest` fails before the body is read.
- `sha256` and `sha512` can be verified. Another algorithm parses (it is valid by the grammar) but
  fetching by it is refused before sending (`UnsupportedAlgorithm`): content is never accepted
  unverified.

## Errors

Every call returns `Result<_, RegistryError>`:

| Variant | Meaning |
|---|---|
| `Input(InputError)` | an input refused before anything was sent |
| `Request { error, meta }` | Concord's failure: build, transport (`error.kind()`), a 2xx body that does not decode, a size `Limit`; `meta` once a response arrived |
| `Api(ApiError)` | a non-2xx answer: `status`, the error document (`errors()`, `has_code(..)`, `ErrorEntry::detail_digest`), the Bearer `challenge` of a `401`, `meta` |
| `Auth(AuthError)` | the token flow: `MalformedChallenge`, `UntrustedRealm`, `InvalidRealm`, `TokenTransport`, `TokenRefused` (with the status), `TokenResponse` |
| `Protocol(ProtocolError)` | a 2xx that breaks the protocol (missing or foreign `Location`, unexpected `Range`, bad `Link`, an unaccepted media type, an unexpected success status), or a server URL refused before use |
| `Integrity(IntegrityError)` | content that does not match its digest or size |

`Display` names the operation (`GET Repository.manifest: registry answered HTTP 404 Not Found
(MANIFEST_UNKNOWN)`) and never contains a token, a credential, a URL or a body text. Error bodies
are read up to 64 KiB (`ErrorBody::TooLarge` beyond). The SDK re-exports `concord` for matching
on `ErrorKind`.

## Limitations

- `ping()`, blob HEAD/GET and manifest GET require `200`, manifest PUT and upload completion require
  `201`, upload start/chunk require `202`, and upload status requires `204`; another 2xx is
  `ProtocolProblem::UnexpectedStatus` on those paths. Catalog, tag-list and token responses
  accept any 2xx before their body and metadata checks.
- A realm whose URL carries a query is refused (`InvalidRealm`): Concord's base URLs take none.
- Tags are validated with the Distribution reference grammar's tag rule
  (`[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}`), which the API page itself does not state.
- Tokens are cached in memory per `Registry` (shared by its clones); two concurrent calls may
  both fetch a token. `issued_at` is not read: the lifetime counts from when the token request was
  sent.
- The supplied HTTP client's redirect policy is outside the SDK's control. Disable redirects or
  reject cross-origin redirects to keep requests and URL query state on authorized origins; the
  SDK's tests use redirects disabled.

## Tests and fixtures

`cargo test -p concord-sdk-distribution-registry` runs offline against loopback servers:
`tests/protocol.rs` (scripted answers, wire-level assertions) and `tests/workflows.rs` (both
workflows against a deterministic fake registry and a separate fake token service, so the
registry and the realm are different origins). The fixtures under `fixtures/` are hand-authored
from the official pages (`PROVENANCE.md`); the fake registry implements the documented behaviour
this SDK relies on. **Neither is vendor conformance:** no Distribution server, public registry or
real token service was contacted.
