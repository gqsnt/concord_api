# Friction log: Distribution registry v2 SDK on frozen Concord (`47a25fbf`)

Author: the SDK author role (Claude Opus subagent). Facts only: what was tried, what was
observed, what was done. Paths are relative to `examples/distribution-registry-sdk/` unless they
start with `crates/` or `docs/`. Cause labels: **Concord** (the library's design or API),
**docs**, **provider** (the registry protocol itself), **SDK** (this crate's choice),
**application/test** (test infrastructure).

Reading material used: `README.md`, `docs/AUTHOR_GUIDE.md`, `docs/CALLER_GUIDE.md`,
`docs/RECIPES.md`, `docs/CHEATSHEET.md`, `crates/concord-pagination/README.md`, Concord's source
for `Prepared`, `Received`, `Incoming`, `ResponseMeta`, `Base` and path encoding, and the GitLab
SDK (`examples/gitlab-sdk`: send policy, `Link` traversal with `Prepared::follow`) and Google
Drive SDK (`examples/google-drive-sdk/src/upload.rs`: a resumable session over `follow`). The
shape of `send`/`api_error`/`ResponseMeta` in this crate follows the GitLab SDK; the session
shape (`needs_status` set before the request is sent) follows the Drive SDK. No library file was
edited.

## What Concord made easy

1. **Server-provided URLs.** `Prepared::follow` (`crates/concord-core/src/prepared.rs:61`) keeps
   the endpoint's method, headers (the Bearer token), body, decoder and operation name, replaces
   only the URL, verbatim, and refuses another origin itself. Every upload request
   (`src/upload.rs:221`, `:279`, `:337`) and every linked catalog/tag page (`src/pages.rs:144`,
   `:189`) is one `follow` call. The opaque `_state=AAA%2Bb%3D%3D&x=1` query arrived byte for
   byte (`tests/protocol.rs` `an_upload_follows_the_latest_location_with_inclusive_ranges`).
   Cause: Concord (positive).
2. **The head before the body, for any status.** `Prepared::receive()` → `Received` gave the
   `401` challenge, `Location`, `Range`, `Link`, `Content-Type` and `Docker-Content-Digest`
   before any body read, and `bytes_limited` / `decode_limited` bounded the error document, the
   token document and the manifest (`src/error.rs:747`, `src/client.rs:230`,
   `src/manifest.rs:289`). A manifest labelled with an unaccepted media type is refused without
   reading its body (`src/manifest.rs:277`). Cause: Concord (positive).
3. **Incremental verification.** `-> stream` without an item returns the unread `Incoming`
   (`src/api.rs:82`); its cancel-safe `chunk()` (`src/blob.rs:186`) fed the SHA-256 hasher and the
   size check, so a too-long body fails at the first byte too many. Cause: Concord (positive).
4. **A runtime credential as a declared value.** `#[sensitive] header(AUTHORIZATION)
   authorization?: HeaderValue` (`src/api.rs:36`, `:55`) plus `maybe_authorization(token)` per
   call put the cached token on the request, flagged sensitive, absent when there is none, with
   `Prepared`'s `Debug` redacting it. No native header insertion was needed. Cause: Concord
   (positive).
5. **The manifest's runtime media type.** `EncodedBody::with_content_type` + `request Encoded;`
   (`src/manifest.rs:354`, `src/api.rs:75`) sent the exact manifest bytes under the media type
   chosen at run time, with `Content-Length`. Cause: Concord (positive).
6. **Pagination.** `concord_pagination::Pages::new` (`src/pages.rs:101`, `:116`) gave
   `next_page`, `items`, `collect_complete`, a resume key and the "an error is never the end"
   rule (`PagesError::Ended` after a refusal, asserted in `tests/protocol.rs`
   `links_that_leave_the_listing_are_refused_before_any_request`) for two listings in ~40 lines of
   adapter each. Cause: concord-pagination (positive).
7. `validate_header_names()` (`src/api.rs` unit test) and the compile-time checks found nothing to
   fix; the first build of the three declarations compiled without an error.

## Workarounds, in order of weight

### F1. The repository name spans path segments; a Concord path value is one segment

- **Observed.** The registry's central value, `<name>` in `/v2/<name>/...`, is one or more
  components joined by `/` (`acme/tools/app`), and the spec's URLs carry the slashes literally.
  A Concord path value "is percent-encoded so that it is always exactly **one** segment (`a/b`
  becomes `a%2Fb`)" (`docs/AUTHOR_GUIDE.md:214`). The documented workaround, the "resource key
  spanning path segments" recipe (`docs/RECIPES.md:190-236`), rewrites the native URL after
  `into_parts()` and "assumes the placeholder is the **last** segment" (`docs/RECIPES.md:227`);
  here the name is in the *middle* of every repository path (`/v2/<name>/manifests/<ref>`), and
  `into_parts()` would also lose `Prepared::follow`, which the uploads need.
- **Done.** The repository endpoints are a separate client whose base URL is the repository:
  `RepoApi::new(http).with_base_url(format!("{registry}/v2/{name}"))` (`src/client.rs:104-112`;
  declaration `src/api.rs:52-104`, its `base` a placeholder at `src/api.rs:57`). The name is
  validated first (`src/reference.rs` `RepositoryName::new`: only `[a-z0-9._-]` and `/`), so
  nothing but segment text reaches the base path. `with_base_url` keeps a path as written, so
  `prepare()` is correct, `follow` works and `Prepared`'s `Debug` stays meaningful.
- **Cost.** Three declarations instead of one (`Registry`, `Repository`, `TokenService`), a
  placeholder base nobody uses, and a repository that is not a declared `scope` (no generated
  `RegistryRepository` handle). Using `with_base_url` to carry a request value is outside its
  documented purpose (another deployment, `docs/CALLER_GUIDE.md` §1).
- **Reproduction.** Declare `scope repository(name: String) { path "v2/{name}"; GET tags() {
  path "tags/list"; } }` and prepare it with `"acme/app"`: the target is
  `/v2/acme%2Fapp/tags/list` (rule at `docs/AUTHOR_GUIDE.md:214`).
- **Cause:** Concord (no multi-segment path value; the recipe covers only a trailing key).

### F2. Digests in paths: Concord escapes `:`; the spec's URLs do not

- **Observed.** Path values are encoded with every character but `A-Z a-z 0-9 - . _ ~` escaped
  (`crates/concord-core/src/__private.rs:122-126`), so a digest segment goes out as
  `sha256%3Aecfc…`. `:` (and `+`, allowed in digest algorithms) are valid path characters
  (RFC 3986 `pchar`), and the spec's URLs show `/v2/<name>/blobs/sha256:…`. Under RFC 3986 an
  escaped reserved character is not equivalent to the character itself, so a strict server or
  proxy may treat the two differently; Distribution itself (Go `net/http` decodes the path) most
  likely accepts both — inferred, not verified against a running server.
- **Done.** `verbatim_last_segment` (`src/client.rs:366-378`) prepares the declared request,
  replaces the last segment with the digest text through `url::PathSegmentsMut` (which leaves `:`
  and `+` alone) and sends it with `Prepared::follow`. Used at `src/blob.rs:56`, `:123` and
  `src/manifest.rs:269`, `:364` (digest references).
- **Side effect.** A followed URL is withheld whole from `Prepared`'s `Debug`
  (`crates/concord-core/src/prepared.rs:256`), so these requests lose Concord's placement-based
  URL redaction in `Debug` (nothing in them is secret).
- **Reproduction.** Replace `verbatim_last_segment(prepared, digest.as_str())` at
  `src/blob.rs:56` with `Ok(prepared)` and run `cargo test -p concord-sdk-distribution-registry
  --test protocol blob_head`: the recorded target is
  `/v2/acme/app/blobs/sha256%3Aecfcbd90…` instead of `/v2/acme/app/blobs/sha256:ecfcbd90…`
  (observed during authoring, then reverted).
- **Cause:** Concord (one encode set for every path value; no per-value opt-out for characters
  that are legal in a segment).

### F3. A send policy cannot answer an authentication challenge

- **Observed.** Every operation needs the same policy: send with the cached token; on a `401`
  Bearer challenge, get a token and send the **same** request again once. A send policy
  (`send Policy;`) is the documented place for "every call of a client" behaviour, but
  `SendWith::send` receives the endpoint by value, `E: Endpoint<T> + Send` without `Clone`
  (`crates/concord-core/src/prepared.rs:235-244`), and `Prepared`/`Received` are consumed by
  sending, so a policy can execute a request once but cannot build it a second time.
- **Done.** No send policy. The SDK keeps a domain method per operation and passes a request
  *factory* to one executor: `Registry::execute(access, |auth| api.clone()
  .maybe_authorization(auth).op(..).prepare())` (`src/client.rs:139-182`), which calls the
  factory again with the new token. Every call site repeats the
  `api.clone().maybe_authorization(auth)` prefix (`src/manifest.rs`, `src/blob.rs`,
  `src/upload.rs`, `src/pages.rs`). The generated `send()` of the declarations is never used.
- **Cause:** Concord (the policy contract has no way to re-prepare; documented scope is "one
  `send()` is one execute", README "The transport boundary"). Whether a retry belongs in Concord is
  a design question; recorded because the challenge-retry is not optional for this provider.

### F4. Endpoints whose URL always comes from the server still need a declared path

- **Observed.** `PATCH`, status `GET` and the completing `PUT` of an upload are always sent to
  the `Location` the registry returned; the declaration still needs a path, so each declares the
  upload collection `blobs/uploads/` as a placeholder (`src/api.rs:91-103`). Calling `prepare()`
  on one of them without `follow` would build a request to the wrong URL; nothing in the
  declaration says "this endpoint's URL is given at run time".
- **Done.** The declarations are private; every use goes through `follow`
  (`src/upload.rs:221`, `:279`, `:337`). The completion adds `digest=<digest>` to the server's URL
  natively with `Url::query_pairs_mut` before `follow` (`src/upload.rs:325`), which appends after
  the server's own query without re-encoding it.
- **Cause:** Concord (minor; `follow` itself made the rest easy).

### F5. Operation names in errors come from the private declaration's identifiers

- **Observed.** The first version named the private clients `RootApi`, `RepoApi`, `TokenApi`;
  error `Display` then read `GET RepoApi.manifest: registry answered HTTP 404 …`, naming a type
  callers cannot see. Concord derives `Operation` from the declared client and endpoint
  identifiers; there is no display name.
- **Done.** The private clients are named after the public types (`Registry`, `Repository`,
  `TokenService`, `src/api.rs:34`, `:53`, `:109`) and imported under aliases
  (`src/client.rs:12`); errors now read `GET Repository.manifest: …` (asserted in
  `tests/protocol.rs` `manifest_answers_that_break_the_contract`).
- **Cause:** Concord/docs (the naming consequence for an SDK that hides its declaration is not
  mentioned in `docs/AUTHOR_GUIDE.md` §5.13, which covers re-exporting the config type).

### F6. A token realm with a query cannot be a base URL

- **Observed.** The token endpoint is the challenge's `realm`, a URL chosen by the registry.
  The SDK uses it as the base of the `TokenService` client (`src/client.rs:206-209`);
  `with_base_url` refuses a query (`crates/concord-core/src/__private.rs:91`), so a realm such as
  `https://auth.example/token?tenant=x` cannot be used and is reported as
  `AuthProblem::InvalidRealm` (`tests/protocol.rs`
  `a_realm_with_a_query_or_user_information_is_refused`). Realms seen in documentation carry no
  query.
- **Cause:** Concord (base URLs take no query; `follow` cannot help because the token request is
  on another origin than any prepared request). Recorded as a limitation in `README.md`.

### F7. Header parsers every SDK writes again

- **Observed.** This crate parses `Link` (RFC 8288, `src/pages.rs:277`) and `WWW-Authenticate`
  (RFC 7235 challenges, `src/auth.rs:270`) by hand; the GitLab SDK has its own `Link` parser
  (`examples/gitlab-sdk/src/link.rs`). Concord deliberately leaves authentication flows and
  pagination to SDKs (README), and `concord-pagination` takes the next key from the SDK.
- **Cause:** SDK/provider (by Concord's stated scope); noted because a strict shared `Link`
  parser would fit `concord-pagination`.

### F8. Test support: a scripted reply cannot name its own server

- **Observed.** An absolute same-origin `Location` or `Link` needs the loopback server's port,
  known only after `TestServer::start` returns, but `Script` replies are built before.
- **Done.** `self_aware` in `tests/protocol.rs:35` builds the script lazily from a `OnceLock`
  holding the URL.
- **Cause:** application/test (`examples/testserver`, not Concord).

## Not friction, for the record

- The SDK's own error type, protocol checks (`Location` origin, `Range` parsing, status per
  operation), digest verification and token cache are provider logic and belong in the SDK.
- `concord::Error` has no public constructor, so protocol and integrity failures are variants of
  the SDK's own error with a `ResponseMeta` beside them; this matched the documented pattern
  (`docs/AUTHOR_GUIDE.md` §5.11).

## Maintenance: redirect policy at the transport boundary

- **Observed.** The caller supplies the `reqwest::Client`; Concord does not change its redirect
  policy (`docs/AUTHOR_GUIDE.md`, “Credentials and redirects”). Redirects happen before the SDK
  can inspect the response. A same-origin check on `Location` or `Prepared::follow` therefore
  cannot by itself prevent a request or URL query from crossing origins through an HTTP redirect.
- **Done.** The SDK's construction examples disable redirects, and its docs state that callers
  must disable them or reject cross-origin hops. Existing callers keep control of their client and
  must configure that policy themselves. No client was rebuilt inside the SDK.
- **Reproduction.** Run `cargo test --locked -p concord-sdk-distribution-registry --test
  protocol authenticated_upload_refuses_foreign_location_and_redirect` with the required Cargo
  environment prefix. With redirects disabled, the test sees the `307`, keeps the upload
  uncertain and records no request at the foreign loopback server. The redirect policy is set in
  `tests/support/mod.rs` `http()`; the SDK cannot enforce it on an arbitrary supplied client.
- **Cause:** Concord/transport boundary (caller-owned redirect policy); also an SDK documentation
  correction. This test does not claim that arbitrary clients are protected.
