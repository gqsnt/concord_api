# Fresh consumer: SDK36 Spotify Web API

Date: 2026-11-07. Live provider calls: **NOT_RUN**. All application traffic must use
`examples/testserver` on loopback. No SDK implementation, existing SDK tests,
`FRICTION.md`, or `PROVENANCE.md` was read before these predictions.

## Predictions

Recorded before writing caller code, from the README and generated public rustdoc.

### Authorization shared by both workflows

- Construct an owned HTTP client with transport retries disabled, app credentials,
  `OAuth`, and `WebApi`. Set both clients' public `with_base_url` to the fixture
  server, preserving the `/v1` path for the Web API. Default production URLs must
  never be sent to.
- `authorize_url(AuthorizeRequest { ... })` only builds `/authorize` with
  `response_type=code`, client ID, redirect URI, space-separated scopes, state, and
  optional dialog selection. It sends nothing; the caller checks returned state.
- `exchange_code(&AuthorizationCode, redirect_uri)` sends `POST /api/token`,
  `Authorization: Basic base64(client_id:client_secret)`, form content type, and
  `grant_type=authorization_code`, `code`, and `redirect_uri`. It returns `Grant`
  with lifetime, optional scopes, and optional refresh token. `oauth.session` uses
  the grant's access token. `oauth.resume` refreshes on the first API call.
- Refresh sends the same Basic header and form content type, with
  `grant_type=refresh_token` and the stored `refresh_token`. A response **omitting**
  `refresh_token` must preserve the existing token; a subsequent forced refresh
  must actually send that original token again. A new nonempty token replaces it.
- Web API GETs carry `Authorization: Bearer <current access token>` and JSON
  `Accept`; they have no body. `prepare()` is unsigned and does not send; use
  ordinary endpoint `send()` so the session policy applies.

### Workflow 1: search artists, select, retrieve

1. Call `pages(api.search_artists("Nina Simone").set_limit(1)
   .set_market(Market::country("FR")?)).max_pages(...)` and collect items with
   `collect_complete()`. Expect `GET /v1/search?q=Nina+Simone&type=artist&market=FR`
   with `limit=1` and offset 0 (explicit or initially absent). The next page uses
   offset 1, retaining search text, type, market, limit and Bearer header.
2. Search returns `ArtistSearch { artists: Paging<Artist> }`; traversal returns
   artists, preserving IDs, names, URIs and available catalog fields. Missing
   popularity/followers become `None`; missing genres/images become empty vectors
   (the rustdoc clarifies the README's broad description of optional fields).
3. Select an artist by its returned typed ID and call
   `api.artist(selected.id.clone()).send()`: `GET /v1/artists/{id}`, no query,
   returning `Artist`.
4. A `next: null` ends traversal without another request, irrespective of `total`.
   A second-page 429 should yield an incomplete collection containing the first
   page's artist, `PagesError::Provider` with the failed offset, status and
   `Retry-After`. Resume with `pages_from(endpoint, failed)`, with no duplicate
   first-page request. A page cap should report incomplete rather than success.

### Workflow 2: list the user's playlists, select, fetch

1. Call `pages(api.me().playlists().set_limit(1)).max_pages(...)` and collect
   completely. Expect `GET /v1/me/playlists?limit=1&offset=0` (initial offset may
   be absent), then offset 1; no user ID is sent in the path. Returns
   `Paging<SimplifiedPlaylist>` per request, or collected simplified playlists.
2. Select a returned ID and call
   `api.playlist(id).get().set_market(Market::country("FR")?).send()`:
   `GET /v1/playlists/{id}?market=FR`, returning `Playlist`, including snapshot,
   owner, optional public/follower data, and the first `tracks` page.
3. Use `playlist.tracks.next_offset()?` and `pages_from` over
   `api.playlist(id).items().set_limit(playlist.tracks.limit).set_market(...)`.
   Expect `GET /v1/playlists/{id}/tracks?market=FR&limit=...&offset=...`.
   Combine the embedded first page with the continuation without duplicates.
   Track, episode, unavailable (`null`) and local-file entries remain distinct.
   `next: null` and `next_offset() == None` mean complete, with no extra probe.
4. A 403 should expose `ApiClass::Forbidden`, status, response metadata, and the
   provider document; `may_be_development_mode()` is a diagnostic hint, not proof
   of quota mode. It must not refresh, erase tokens or retry. Continue with a
   later allowed call using the same token. A quota 429 with
   `reason=QUOTA_EXCEEDED` is distinct from ordinary rate limiting.

### Failure and secret expectations

- Non-2xx statuses become `Error::Api` with authoritative HTTP status, provider
  status/message/reason when readable, and metadata. A malformed error document
  does not hide its HTTP status. 400/404/5xx are not automatically retried.
- Ordinary 429 is not retried by default; valid `Retry-After` is exposed as a
  duration. Configured retries are bounded and quota 429 is never retried.
- A 401 refreshes and replays a GET once; a second 401 is Unauthorized.
- OAuth refusals use `Error::Auth` and the RFC 6749 document. `invalid_grant`
  requires reauthorization and suppresses later refresh attempts; other refusals
  preserve the token. Invalid token documents are protocol errors.
- Malformed 2xx JSON is a Concord Decode error; wrong response media type is
  Metadata; oversized/truncated bodies are Limit/Transport errors. A mismatched
  page offset or zero-limit continuation is a Protocol error inside pagination.
- Invalid IDs (including Spotify URIs passed as IDs) fail before network use.
- Errors' Display/Debug, credentials, grants, sessions and generated clients must
  not print fixture secrets, authorization codes, state, provider messages or
  URLs. Explicit token exposure is reserved for persistence assertions; the
  authorization URL necessarily carries state and client ID.
- Successful ordinary calls return models, not a response wrapper; inspect paging
  metadata on models and response-head metadata through failures.

## Documentation and action log

1. Read `examples/spotify-web-api-sdk/README.md` first.
2. Generated rustdoc with
   `env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4 cargo doc -p concord-sdk-spotify-web-api --no-deps --offline`
   (first build without `--locked`, exit 0). Output is in the worktree's default
   `target/`. Initial `git status --short` was empty; no `AGENTS.md` was found
   within this worktree. No parent-directory material was read.
3. Read the generated SDK crate index, OAuth, Session, SessionOptions, WebApi,
   endpoint types, AppCredentials, Grant, RefreshToken, AuthorizeRequest,
   AuthorizationCode, State, model types, error types, Paging, pages, pages_from,
   and Listed. Did not follow rustdoc Source links.
4. Read these exact official pages on **2026-11-07**, using read-only browsing:
   - [API calls](https://developer.spotify.com/documentation/web-api/concepts/api-calls):
     authorization, status/error documents and offset/limit pagination.
   - [Refreshing tokens](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens):
     Basic-authenticated form refresh and retention when a replacement is absent.
   - [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes):
     allowlisted development users, possible 403, and quota 429 reason.
5. Wrote only this file and the new caller application
   `tests/first_use.rs`. Its two Tokio tests use `TestServer::start` responders,
   synthetic artist/playlist data, bounded HTTP timeouts, disabled proxies,
   disabled redirects and disabled transport retries. Both OAuth and WebApi
   point at loopback; recorded Host headers are asserted to match the server.
   Fixture `next`/`href` URLs are metadata, never provider requests.
6. Ran the targeted test four times while developing it (the first three
   diagnostics are below), then formatted only the new test file with
   `rustfmt --edition 2024`. Ran the full package suite once, then checked the
   new file with `rustfmt --check --edition 2024`.
7. Verified `git status --short` contains only the two new files. Root manifest,
   lockfile, all tracked source, shared crates/checks/scripts and other examples
   are unchanged. The package was already resolvable in the workspace; no
   membership change, dependency addition or lockfile update was needed. No new
   crate, version or license justification is necessary. Did not read or edit
   `PROVENANCE.md`. No commits were attempted.

## First working call

On the third invocation of the first-use test (after correcting the fixture base
URL), `search.clone().send().await` returned Nina Simone in an `ArtistSearch`.
It took the public `with_base_url(server.url())` on both clients, existing
dependencies, `OAuth::resume` with a stored refresh token, and the generated
`search_artists(...).set_limit(1).set_offset(0).set_market(...).send()` call.
The fixture recorded one Basic-authenticated refresh with no replacement refresh
token, followed by the Bearer-authenticated `/v1/search` request. All first-call
model and token-retention assertions passed. The same run proceeded through 429
partial collection/resume, selected artist retrieval with 401 refresh/replay,
cap handling, malformed JSON and media-type assertions, before a later page
offset expectation failed. No SDK source, pre-existing tests, friction or
provenance files were read before this working call (or afterwards).

## Friction incidents (in order)

1. The README does not show a loopback override. Read public rustdoc for
   `OAuth::with_base_url` and `WebApi::with_base_url`; both are available.
   Read the SDK and fixture package manifests solely for dependency names; all
   needed dependencies were already declared. Generated additional public
   rustdoc for `concord-example-testserver`, `concord-pagination`, `concord-core`
   with `--locked --offline --no-deps`. Read the pagination README and public
   rustdoc for fixture types, Incomplete/PagesError/Resume/Progress,
   ResponseMeta/Error/ErrorKind/Operation. No implementation was read.
2. The first caller compiled but warned about an unused `PagesError` import.
   Removed it: `into_provider_error()` and `outcome()` avoid naming variants for
   the partial-result assertions. This was a caller import mistake.
3. The first runtime search failed as `ApiClass::Server`/500 because the fixture
   handler did not match its path. Generated WebApi rustdoc describes the base as
   `https://api.spotify.com/v1`, and says declared paths append to an override.
   Predicted that the override should preserve `/v1`. Added a failure
   diagnostic containing only recorded methods and targets; the second run
   showed `POST /api/token`, then `GET /v1/v1/search?...`. Corrected the override
   to the bare `server.url()`. No source lookup. The value-free SDK error was
   appropriately redacted but could not diagnose the duplicated fixture path;
   inspecting the fixture's recorded request did.
4. On that third run, `pages(api.search_artists("bad-offset").set_offset(0))`
   accepted a response with offset 99 and reported complete (`Ok([])`). This
   contradicts the README's statement that each page is checked against its
   requested offset. Kept this as an explicit observation assertion, and added
   the same fixture through `pages_from(..., Resume::At(0))` to test whether
   explicit resume validation behaves differently. It returned
   `Error::Protocol` inside `PagesError::Provider` as predicted. The request
   explicitly setting offset 0 and the resume request receive the same offset
   99 fixture. Source remained unopened. The passing test intentionally asserts
   both observed behaviors; it does not claim the initial-page contract is met.

No surprising generated endpoint names or mandatory setter-trait imports were
encountered. `collect_complete`, bounded outcomes and resume accessors made
completion clear after consulting public pagination docs. Ordinary success
models expose paging fields; failure metadata is accessible without bypassing
the session policy or naming Concord's internal receive types.

## Prediction comparison

| Prediction | Observed behavior in the new caller tests |
|---|---|
| Override WebApi with a base ending `/v1` | **Mismatch:** that duplicates the prefix; a bare loopback origin produces the declared `/v1/...` targets. OAuth uses the same bare origin. |
| Authorization URL is built without traffic | Matched: path, all six query pairs (including requested dialog), loopback host and zero recorded requests asserted. Callback state comparison is explicit caller code. |
| Authorization-code exchange is a Basic-authenticated form | Matched: exact POST target, Basic header, content type and decoded form pairs asserted. Grant lifetime, scope and refresh token checked. |
| Resumed session refreshes before its first GET | Matched: the first recorded request is the refresh form, then a Bearer-authenticated search. |
| Missing replacement refresh token retains the old token | Matched: two successful refresh documents deliberately omit it; `session.refresh_token()` stays equal to the stored value, and **both** recorded refresh forms actually send that original value. Scope also survives a later response omitting scope. |
| Search query/GET headers and selected artist target | Matched: exact decoded query pairs, offsets 0/1, retained market/type/query/limit, current Bearer token, JSON Accept, empty body, no query on selected artist GET. Query ordering was intentionally not treated as a contract. |
| Search output and restricted catalog fields | Matched: typed IDs, selected name/genres, first-page offset/limit/total/next-offset; omitted popularity/followers are None, omitted genres/images are empty vectors. |
| 429 interrupts with a prefix and resumable offset | Matched: one collected artist, failed `Resume::At(1)`, counters (2 admitted, 1 success, 1 failure), RateLimited, status 429, provider document, response operation/request-ID and seven-second Retry-After. No default retry. Explicit resume fetches just offset 1. |
| Terminal page is complete without a probe; cap is incomplete | Matched: a one-page cap exactly on the final page succeeds with one wire request; a cap before the last page retains the prefix and reports Bounded with resume 1. |
| 401 refreshes and replays a GET once | Matched: selected artist is attempted with A1, refreshed, replayed with A2; exactly two total refresh requests across the entire artist task. A final second 401 was not injected by the new tests. |
| Every requested page offset is validated | **Mismatch:** initial `pages(request.set_offset(0))` accepts offset 99 and returns complete/empty. Explicit `pages_from(request, Resume::At(0))` rejects the identical response as Protocol. |
| User playlist listing, selected playlist and continuation targets | Matched: `/v1/me/playlists` offsets 0/1; selected ID used in `/v1/playlists/{id}?market=FR`; embedded offset 0/limit 2/total 4, snapshot and owner; continuation `/tracks` at offset 2 retaining market/limit. |
| Playlist public/follower fields may be unavailable | Matched: both can be absent; no extended access assumed. |
| 403 can indicate development mode, without refresh/retry | Matched: the listing's second page preserves its first playlist and returns Forbidden, hint true, provider document, response metadata and failed offset 1. Exactly three requests (exchange, first page, forbidden page) have occurred. A deliberate later resume succeeds with the same access token. The fixture does not prove a real quota mode. |
| Quota 429 differs from ordinary rate limiting | Matched: QUOTA_EXCEEDED class/document and zero Retry-After exposed; no retry even with two retries configured and a valid zero delay. Embedded track/episode prefix survives, failed continuation offset 2 is manually resumed. |
| Playlist media and terminal continuation | Matched: exactly four combined entries in order, track, episode, null/unavailable, local track with no track/artist ID. Final continuation succeeds at a one-page cap and sends no terminal probe. Raw terminal `next_offset() == None` was not separately asserted. |
| Non-2xx status and provider documents/metadata | Matched for readable 404, 403 and both 429 classes. An HTML 403 remains Forbidden with document Metadata error; its HTTP status stays authoritative. New callers did not inject 400/5xx Web API responses or mismatched HTTP/body status numbers. |
| Malformed 2xx / media-type errors retain response context | Matched: Decode and Metadata respectively, status 200, endpoint identity, and request-ID via `head_meta()` after Decode. No extra retries. Truncated/oversized bodies were not injected by the new callers. |
| OAuth invalid_grant requires reauthorization | Matched: RFC 6749 code/document and status 400, response metadata, token removed; the next call is ReauthorizationRequired with no new request. Other token refusals, bad token contracts, rotation and replace_grant were not independently exercised here. |
| Invalid IDs fail before network use | Matched for a Spotify URI supplied as an ID; request count unchanged. |
| Formatting withholds secrets/provider text/URLs | Tested fixture credentials, stored/access/code tokens, authorization code, state, provider messages and response header values stay out of the relevant Debug/Display output. Explicit document/header/token access remains available for application decisions/storage. Authorize URL intentionally exposes state/client ID. No claim about arbitrary third-party formatter chains. |
| Deadline, valid automatic rate-limit wait, and other bounds | Not independently exercised by these two new callers. They use a client timeout; no SDK session deadline is configured. The existing full package tests also ran, but their source was not used to construct this application. |

## Suggested changes, not applied

- Concord's generated client/base-URL documentation should make the declared
  origin and appended `/v1` endpoint prefix unambiguous, with a bare-origin
  loopback example. This is a public generated-doc issue; no shared macro/core
  implementation was inspected or changed.
- The SDK pagination surface should validate the initial response against an
  explicit request offset, as its README promises, or document the limitation
  and recommend an explicit `pages_from` resume for that check. This observation
  is recorded rather than editing the SDK's `src/`.
- A brief README example retaining `Incomplete.collected` and extracting
  `(error, failed, progress)` would help a caller preserve partial results; the
  existing happy-path example deliberately discards that prefix with `map_err`.
  This was resolved through public pagination documentation, not source access.

## Validation

All Cargo commands used `env -u CARGO_TARGET_DIR CARGO_BUILD_JOBS=4`, one command
at a time, building into this worktree's default `target/`. The first documentation
build ran offline without `--locked`; every later Cargo invocation included
`--locked --offline`.

| Command / stage | Result |
|---|---|
| SDK rustdoc, `cargo doc -p concord-sdk-spotify-web-api --no-deps --offline` | Passed; no lock change. |
| Public fixture/pagination/core rustdoc, `cargo doc --locked --offline -p concord-example-testserver -p concord-pagination -p concord-core --no-deps` | Passed. |
| Targeted first-use invocation 1 | Compiled; unused import warning; failed on fixture 500 from duplicated `/v1`. |
| Targeted invocation 2, with recorded-target diagnostic | Failed on the same 500; exposed `/v1/v1/search`. |
| Targeted invocation 3, corrected base URL | First working call and connected artist workflow passed; later initial-offset assertion failed because the SDK accepted offset 99. |
| Targeted invocation 4, both caller tasks | **2 passed**, no warnings. |
| `cargo test --locked --offline -p concord-sdk-spotify-web-api` after formatting | **28 tests passed** (2 new caller, 16 existing protocol, 10 existing workflow), **2 doctests passed**; 0 failures. |
| `rustfmt --check --edition 2024 examples/spotify-web-api-sdk/tests/first_use.rs` | Passed. |
| Working-tree scope / tracked diff check | Only FIRST_USE.md and tests/first_use.rs added; tracked source, Cargo.toml and Cargo.lock unchanged. |

**Live provider calls: NOT_RUN.** Reading developer documentation was the only
external research; no Spotify API/accounts request was made.

## Assistance label

**docs-only**. Used the README, generated public SDK/Concord/fixture rustdoc,
Concord's public pagination README, the three official Spotify pages above, and
package manifests for dependency names. No SDK implementation, existing test
source, FRICTION.md, PROVENANCE.md, or private campaign material was read. Test
execution and compiler diagnostics do not represent source lookups. No
sub-agents, implementation assistance or additional dependencies were used.
