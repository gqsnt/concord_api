# Provenance

Live provider calls: **NOT_RUN**. No request in this package's tests leaves 127.0.0.1; every
response comes from a fixture below or from an inline synthetic body in the test that uses it.

## Documentation read

All three pages were fetched read-only with `curl` on **2026-11-07 at 06:16 UTC**; the HTML and
extracted text were kept outside the package (worktree `target/tmp/docs/`, not committed).

| Page | SHA-256 of the HTML as fetched | What the SDK takes from it |
|---|---|---|
| <https://developer.spotify.com/documentation/web-api/concepts/api-calls> | `ad642762d3280498bd5da8a610ffaf4ed62943ab8ba76e84ebcb1bb3dab112d5` | base address `https://api.spotify.com`; JSON responses; the status table (200–503, 429 "rate limiting or quota restrictions", 503 may be resent); the two error formats: the RFC 6749 authentication error object (`error`, `error_description`, example `invalid_client` / `Invalid client secret`) and the regular error object (`{"error": {"status", "message", "reason"?}}`, `reason` e.g. `QUOTA_EXCEEDED`, example 400 `invalid id`); offset/limit pagination |
| <https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens> | `f88924f93354c12b5d66b620f7e405319b6e19a685ba21119a7acf6bf03acb87` | the token response fields (`access_token`, `token_type` `Bearer`, `scope`, `expires_in` 3600, `refresh_token`); access tokens last 1 hour; the refresh request (`POST https://accounts.spotify.com/api/token`, form `grant_type=refresh_token`, `refresh_token`, `Content-Type: application/x-www-form-urlencoded`, `Authorization: Basic base64(client_id:client_secret)` for the authorization-code flow); "a refresh token might not be included in each response. When a refresh token is not returned, continue using the existing token"; `invalid_grant` means expired, revoked or invalid, and the app should discard the refresh token and start the authorization flow "instead of retrying the refresh request"; refresh tokens last 6 months from authorization and refreshing does not extend them |
| <https://developer.spotify.com/documentation/web-api/concepts/quota-modes> | `a9abb43e9e8468ed015a10585fba2e3fd4de512dd39621ca106c764f8a3ad3f8` | development mode (at most 5 allowlisted users, owner needs Premium); quota buckets and the 429 body with `reason: QUOTA_EXCEEDED` (verbatim example); a user who is not allowlisted "will receive a 403 status code error"; extended quota mode is not assumed |

### Not read (the brief allows only the pages above)

These facts come from the author's prior knowledge of Spotify's public reference and of the OAuth
standard, were **not verified in this run**, and are the first things to check against the
reference before any live use:

| Fact used | Where it would be documented |
|---|---|
| `GET /v1/search` with `q`, `type=artist`, `market`, `limit`, `offset`, and the response `{"artists": <paging object>}` | Web API reference, Search |
| `GET /v1/artists/{id}` and the artist object fields (`id`, `name`, `uri`, `genres`, `popularity`, `followers.total`, `images`, `external_urls.spotify`) | Web API reference, Get Artist |
| `GET /v1/me/playlists` (`limit`, `offset`), the simplified playlist object, the `playlist-read-private` / `playlist-read-collaborative` scopes | Web API reference, Get Current User's Playlists; scopes page |
| `GET /v1/playlists/{id}` (`market`), its embedded `tracks` paging object of playlist items (`added_at`, `added_by`, `is_local`, `track` that may be `null`, a track or an episode by `type`), and `GET /v1/playlists/{id}/tracks` for further items | Web API reference, Get Playlist / Get Playlist Items |
| the paging object fields `href`, `items`, `limit`, `next`, `offset`, `previous`, `total`, and `next` being `offset + limit` | Web API reference (paging object) |
| the authorization URL `https://accounts.spotify.com/authorize` with `client_id`, `response_type=code`, `redirect_uri`, `scope` (space-separated), `state`, `show_dialog`; the code exchange form `grant_type=authorization_code`, `code`, `redirect_uri` with the same Basic credential | Authorization code flow tutorial; RFC 6749 §4.1.1 and §4.1.3 |
| `Retry-After` on a rate-limited 429 (the brief's contract note says so; the api-calls page gives the status only) | Rate limits page; RFC 9110 §10.2.3 |
| base-62 IDs (the SDK accepts 1–64 ASCII alphanumerics, a deliberately loose bound) | Spotify URIs and IDs concept page |

Because of this, every Web API **success** fixture is labelled synthetic: its shape is the
author's, not a documented example.

## Fixtures

"Verbatim" means the documented example's JSON unchanged; "derived" means the documented example
with synthetic values substituted as stated; "synthetic" means authored for this package.

| Fixture | Kind | Source and changes |
|---|---|---|
| `token-grant.json` | derived | refreshing-tokens page, authorization token response example; token values replaced by `synthetic-access-1` / `synthetic-refresh-1`, scope replaced by the two playlist scopes this slice requests |
| `token-refresh-rotated.json` | derived | refreshing-tokens page, refresh response example; values replaced by `synthetic-access-rotated` / `synthetic-refresh-2`, scope as above |
| `token-refresh-kept.json` | derived | the same example with `refresh_token` removed, as the page says a refresh response may omit it |
| `oauth-invalid-client.json` | verbatim | api-calls page, authentication error object example (the page shows no status; the tests send it with 401, the status RFC 6749 §5.2 gives a failed Basic client authentication, from the author's knowledge) |
| `oauth-invalid-grant.json` | derived | error code `invalid_grant` from the refreshing-tokens page; `error_description` synthetic |
| `error-400.json` | verbatim | api-calls page, regular error object example (400 `invalid id`) |
| `error-429-quota.json` | verbatim | quota-modes page, quota exceeded example |
| `error-401.json`, `error-403.json`, `error-404.json`, `error-429-rate.json`, `error-500.json` | synthetic | the regular error object's documented shape with the status of the class; messages invented |
| `proxy-502.html`, `wrong-media.html` | synthetic | an HTML page from an intermediary, to exercise unreadable error documents and the `Json` media check |
| `malformed.json` | synthetic | a search response cut mid-string |
| `search-artists-1.json`, `search-artists-2.json` | synthetic | two pages (limit 2, total 3) of a search; the second artist object omits `popularity`, `followers` and `genres` and has `images: null`, to exercise the optional fields |
| `artist.json` | synthetic | the selected artist's full object |
| `me-playlists-1.json`, `me-playlists-2.json` | synthetic | two pages (limit 2, total 3) of the user's playlists; one owner without a display name, a `public: null` |
| `playlist.json` | synthetic | the selected playlist with its first item page (limit 4, total 6): a track, an episode, a `null` track, a local file without IDs |
| `playlist-items-2.json` | synthetic | the remaining two items (offset 4) |

Inline bodies in `tests/protocol.rs` (token documents breaking the contract, an empty
`refresh_token`, an oversized lazily generated body, a truncated body, `Retry-After` values) are
synthetic and labelled where they are written. No expected value in a test was copied from the
SDK's output: the expected wire strings were written by hand (the Basic credential was computed
with `base64` independently of the SDK).

## Dependencies

No new crate: `Cargo.lock` gains only this package's own entry. It uses workspace crates
(`concord` with feature `form`, `concord-auth`, `concord-pagination`, `concord-retry`), `serde`
(derive) and `tokio` (`time`, for the retry sleep), all already in the lock; dev-dependencies
`concord-example-testserver`, `serde_json` and `tokio` (`macros`, `rt-multi-thread`).

## Maintenance ticket (2026-11-07)

Re-read the exact [API calls](https://developer.spotify.com/documentation/web-api/concepts/api-calls),
[Refreshing tokens](https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens)
and [Quota modes](https://developer.spotify.com/documentation/web-api/concepts/quota-modes) pages
read-only on **2026-11-07**, plus
[Add Items to Playlist](https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist)
on that date. No additional provider pages or live endpoints were used.

The add reference specifies POST `/playlists/{playlist_id}/tracks`, a JSON URI array of up to
100 tracks/episodes, optional zero-based position (otherwise append), write scopes
`playlist-modify-public` / `playlist-modify-private`, and a 201 snapshot document. It currently
marks the route deprecated. The ticket's `/tracks` route is retained. The SDK's strict
22-character base-62 catalog-URI check is a conservative validation choice consistent with the
reference's examples, not a claim that this page specifies a universal ID-length rule.

`fixtures/playlist-snapshot.json` is derived from the documented `snapshot_id` response shape,
with a synthetic opaque version. New inline bodies in `tests/add_items.rs` and
`tests/add_items_consumer.rs` are synthetic: they simulate writes, permissions, post-write
playlist state, failures and token responses. The track/episode URI values in the new tests
are reference examples; they are only strings sent to loopback. Timeouts and no write replay
are SDK recovery policy required by the ticket, not a provider idempotency guarantee.

No dependencies, features, manifest membership or lock entries changed during maintenance:
the package was already a workspace member and already present in `Cargo.lock` at entry.
The first baseline test used `--offline` without `--locked`; subsequent Cargo commands use
`--offline --locked`, `CARGO_BUILD_JOBS=4`, and the default worktree `target/`.

Installed `reqwest` 0.13.4 registry sources (`src/async_impl/{request,client}.rs`, `src/retry.rs`)
were inspected for transport policy: retry/redirect configuration belongs to the supplied
client. The caller contract therefore explicitly requires no transport retries or redirects
when using playlist additions.

## Examples consulted

This SDK is **example-assisted**. Files read, and what was taken from each:

| File | Taken |
|---|---|
| `examples/xero-accounting-sdk/Cargo.toml` | the dependency set and features of an OAuth-refresh SDK (`concord` with `form`, `concord-auth`, `concord-pagination`, `concord-retry`, `tokio`) |
| `examples/xero-accounting-sdk/src/auth.rs` | `TokenCache<(), E>` with a unit key and capacity 1 for one principal; an acquisition closure that captures its inputs but never the cache (Arc cycle); mapping `RefreshError::Acquire(Arc<E>)` into the SDK error; a private `api!` token client (`request Form`, `response Json`, `#[sensitive] header(AUTHORIZATION) credential: HeaderValue`, `limit`) |
| `examples/xero-accounting-sdk/src/policy.rs` | `Retry::run` with an async attempt factory over `request.try_clone()`, applying the snapshot header, `decoder.execute`, invalidating the used generation on 401; a `retry_after` helper over `concord_retry::http::retry_after(.., SystemTime::now())`; reading the error document with `received.document::<Json, _>(limit)` |
| `examples/testserver/src/lib.rs` | the fixture server's API (`TestServer`, `Reply`, `Body`, `Script`, `Gate`, `Recorded`) |

Concord documentation and sources read: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
`docs/CHEATSHEET.md`, `docs/RECIPES.md`; the READMEs of `concord-auth`, `concord-retry`,
`concord-pagination`, `concord-ratelimit`, `concord-codecs`; `crates/concord-auth/src/cache.rs`,
`crates/concord-retry/src/{lib,http}.rs`, `crates/concord-pagination/src/{pages,types}.rs` (parts),
`crates/concord-core/src/{prepared,error}.rs` (signatures).
