# concord-sdk-spotify-web-api

The declaration owns the provider error document: `error crate::ErrorEnvelope in concord::Json;`, `error crate::error::OAuthErrorObject in concord::Json;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

A bounded SDK for the [Spotify Web API](https://developer.spotify.com/documentation/web-api)
built on Concord. It covers two connected workflows of an authorized user, and the OAuth
authorization-code flow with refresh that they need, plus adding items to an owned playlist.
Every test runs against local fixtures;
**the real service was never called** (live provider calls: NOT_RUN).

| Workflow | Calls |
|---|---|
| Search for artists and retrieve the selected one | `GET /v1/search?type=artist` (offset pages), `GET /v1/artists/{id}` |
| List the user's playlists and fetch the selected one | `GET /v1/me/playlists` (offset pages), `GET /v1/playlists/{id}`, `GET /v1/playlists/{id}/tracks` (the rest of its items) |
| Add items to an owned playlist | `POST /v1/playlists/{id}/tracks`, returning `PlaylistSnapshot.snapshot_id` |
| Authorization | the authorization URL (built, never sent), `POST accounts/api/token` with `grant_type=authorization_code` and `grant_type=refresh_token` |

## Calling it

```rust,ignore
use concord_sdk_spotify_web_api::{
    AppCredentials, AuthorizeRequest, ClientSecret, OAuth, SessionOptions, State, WebApi,
    WebApiConfig, pages,
};

let http = concord::reqwest::Client::new(); // timeouts, proxies, TLS: your client
let app = AppCredentials::new(client_id, &ClientSecret::new(client_secret))?;
let oauth = OAuth::new(http.clone(), app);

// 1. Send the user to the authorization page (nothing is sent by the SDK).
let url = oauth.authorize_url(&AuthorizeRequest {
    redirect_uri: "https://app.example/callback".into(),
    scopes: vec!["playlist-read-private".into(), "playlist-read-collaborative".into()],
    state: State::new(csrf_state),
    show_dialog: false,
})?;

// 2. On the redirect, check `state` yourself, then exchange the code.
let grant = oauth.exchange_code(&code, "https://app.example/callback").await?;
let session = oauth.session(grant, SessionOptions::new());
// ...or later, from a stored refresh token: oauth.resume(refresh_token, SessionOptions::new())

let api = WebApi::new(http, WebApiConfig { session: session.clone() });

// Workflow 1: search (every page, bounded), select, retrieve.
let artists = pages(api.search_artists("Nina Simone").set_limit(20))
    .max_pages(std::num::NonZeroUsize::new(5).unwrap())
    .items()
    .collect_complete()
    .await
    .map_err(|incomplete| incomplete.error)?;
let artist = api.artist(artists[0].id.clone()).send().await?;

// Workflow 2: the user's playlists, select, fetch, then the rest of its items.
let playlists = pages(api.me().playlists().set_limit(50)).items().collect_complete().await
    .map_err(|incomplete| incomplete.error)?;
let playlist = api.playlist(playlists[0].id.clone()).get().send().await?;
if let Some(offset) = playlist.tracks.next_offset()? {
    let rest = pages(api.playlist(playlist.id.clone()).items().set_limit(playlist.tracks.limit))
        .starting_at(concord_sdk_spotify_web_api::concord_pagination::Resume::At(offset))
        .items().collect_complete().await.map_err(|i| i.error)?;
}

// Persist the refresh token after use: the provider may have rotated it.
store(session.refresh_token());
```

The generated client is the SDK's client: `api.search_artists(q)`, `api.artist(id)`,
`api.me().playlists()`, `api.playlist(id).get()` and `api.playlist(id).items()` are Concord
endpoints with their setters (`set_limit`, `set_offset`, `set_market`), `prepare()` and
`send()`. `send()` returns `Result<T, concord_sdk_spotify_web_api::Error>`. `prepare()` builds the
declared request **without** `Authorization` (the session adds it when sending).

`pages(first)` walks any listing (`search_artists`, `me().playlists()`, `playlist(id).items()`)
with `concord-pagination`: `offset + limit` while the provider returns a `next` link, each page
checked to start at the requested offset. It reads nothing until polled, has no page cap unless
you set `max_pages`, and a failed page ends it with `PagesError::Provider { failed, .. }`:
`pages(first).starting_at(failed)` resumes there.

Use `concord_sdk_spotify_web_api::concord_pagination::Resume` for the position. `Resume::Start`
repeats `first` unchanged, including its configured offset and limit; `Resume::At(key)` takes
the `u32` result offset in a returned page's `next` or `Paging::next_offset()`.

Inputs are typed: `SpotifyId::new` accepts 1–64 ASCII letters and digits (a `spotify:` URI or
a URL is refused before any request), `Market::country("SE")` / `Market::from_token()`.

## Adding playlist items

Authorize with `playlist-modify-public` or `playlist-modify-private` for the playlist being
changed. Select a playlist owned by that user; Spotify checks ownership and access. Existing
read-only grants do not gain write permission automatically.

```rust,ignore
use concord_sdk_spotify_web_api::AddItems;

let items = AddItems::new([
    "spotify:track:4iV5W9uYEdYUVa79Axb7Rh",
    "spotify:episode:512ojhOuo1ktJprKbVcKyQ",
])?.position(0); // zero-based insertion; omit position() to append
let snapshot = api.playlist(owned_playlist_id).add_items(items).send().await?;
store_snapshot(snapshot.snapshot_id);
```

`AddItems` validates 1–100 URIs before any HTTP, including refresh. Its fields are private, so
generated setters also require a validated body. The SDK accepts track/episode catalog URIs
with 22 ASCII base-62 ID characters; it rejects URLs, local-file URIs and other item types.
Order and duplicates are preserved; batches are never split automatically. Position is an
unsigned index whose validity for the current playlist is checked by Spotify.

Writes acquire and use the same session token as reads but are sent once, including after a
401 or 429. A write 401 invalidates the token for the next caller-initiated call. A dispatched
write with a timeout, transport failure or unreadable success result returns
`Error::UncertainWrite(cause)`: the items may already be present. Re-read the playlist, including
its item pages as needed, and reconcile before choosing another addition. A token-acquisition
failure before dispatch does not imply that items were added. The write's session deadline
covers token acquisition, dispatch and response-body decoding. If the caller cancels the
future externally after dispatch, it must also reconcile the playlist.

Use an HTTP client with `retry(reqwest::retry::never())` and
`redirect(reqwest::redirect::Policy::none())` for this single-send contract: the supplied
transport's internal retries and redirects are outside the session's control. `prepare()`
builds an unsigned request; use the generated endpoint's `send()` to apply session rules.

Provider reference, read **2026-11-07**:
[Add Items to Playlist](https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist).
It marks `/tracks` deprecated; this SDK implements the ticket's specified route. No live
availability for development-mode accounts is assumed.

## Failure semantics

| Situation | What the SDK does | What you get |
|---|---|---|
| Access token near expiry (60 s margin, configurable) | one refresh, shared by every concurrent request of the session | the call proceeds |
| Refresh response without `refresh_token` (or with an empty one) | the stored refresh token **is kept** | `session.refresh_token()` is unchanged |
| Refresh response with a new `refresh_token` | it replaces the stored one | persist `session.refresh_token()` |
| `invalid_grant` from the token endpoint | the refresh token is marked rejected and never sent again | `Error::Auth(AuthError::InvalidGrant)`, then `AuthError::ReauthorizationRequired` without any request; `session.replace_grant(new_grant)` after the user authorizes again |
| Other token endpoint refusal (`invalid_client`, 5xx, unreadable document) | nothing is erased | `Error::Auth(AuthError::Refused)` with the RFC 6749 document when readable |
| Token document that breaks the contract (not `Bearer`, `expires_in` 0 or within the refresh margin, an access token that is not a header token) | refused | `AuthError::Protocol` |
| 401 | invalidates exactly the token that request used; refreshes and replays a `GET` once; returns a write's first 401 without replay | `ApiClass::Unauthorized` if final |
| 403 | **no** refresh, **no** invalidation, no retry | `ApiClass::Forbidden`; `error.may_be_development_mode()` is `true`: for an app in development mode a user who is not on the app's allowlist gets a 403 (it can also be any other refusal; the SDK cannot tell them apart) |
| 429 | not retried by default | `ApiClass::RateLimited` with `error.retry_after()` (strict `Retry-After`: seconds or IMF-fixdate; repeated, signed or invalid values are reported, not guessed) |
| A read's 429 with retries enabled (`SessionOptions::rate_limit_retries(n)`) | waits `Retry-After` (a minimum) when it is valid and ≤ `max_retry_wait` (5 s default), at most `n` times; writes never retry | success, or the last 429 |
| 429 with `reason: QUOTA_EXCEEDED` | never retried | `ApiClass::QuotaExceeded` |
| 400, 404, 5xx, any other non-2xx | not retried | `ApiClass::{BadRequest, NotFound, Server, Other}`; the error object (`status`, `message`, `reason`) when readable (≤ 64 KiB, `application/json`), otherwise the reason it is not; the status is authoritative either way |
| 2xx with another media type, a malformed, truncated or oversized (> 4 MiB) body | — | `Error::Concord` (`ErrorKind::Metadata`, `Decode`, `Transport`, `Limit`) |
| A page that does not start at the requested offset, or a `next` link with `limit` 0 | the traversal stops | `Error::Protocol` inside `PagesError::Provider` |
| `SessionOptions::deadline(d)` expires before any attempt completes | — | `Error::Deadline` |
| Dispatched addition times out or loses its success result | no retry or refresh; caller re-reads the playlist | `Error::UncertainWrite`; original Concord cause via `error.concord()` or boxed error |

`Display` of every error is static text, a status code and, for a Concord failure, Concord's own
value-free description; provider messages, tokens, codes and URLs are never formatted. `Debug` of `AppCredentials`, `ClientSecret`, `RefreshToken`,
`AuthorizationCode`, `State`, `Grant`, `OAuth`, `Session` and the generated client never shows a
secret (tested). The authorization URL necessarily contains `state` and the client ID.

Development mode: newly created apps start in development mode, with at most 5 allowlisted users
and request quotas shared per quota bucket. The models therefore make every field that a given
app or account may not receive optional (`popularity`, `followers`, `genres`, `images`, playlist
`public`), and the SDK never assumes extended quota access.

Transport: you own the `reqwest::Client`. For exact request accounting give it
`retry(reqwest::retry::never())`; the SDK's own repetition is the single 401 replay and the
opt-in 429 retries, both only for `GET`.

## Excluded

Playback control, audio download, recommendations, writes other than playlist item additions,
the PKCE and client-credentials flows, conditional requests (`If-None-Match`/304), automatic
5xx retries, persistence of tokens (you store `session.refresh_token()`), and any assumption that
an account or app has extended quota access.

## Tests

```sh
cargo test --locked -p concord-sdk-spotify-web-api
```

`tests/protocol.rs`: request shapes on the wire (authorization URL, Basic-authenticated token
forms, every Web API target), every documented status class with its document, the token
endpoint's error documents, strict `Retry-After`, the token document contract, malformed /
empty / truncated / oversized / wrong-media / unlabelled bodies, paging contract violations, the
session deadline, input validation, redaction. `tests/workflows.rs`: both workflows end to end (two pages each, a
playlist with a track, an episode, an unavailable item and a local file), refreshes with and
without a replacement refresh token, single-flight refresh under concurrency (also for two
concurrent 401s on the same token), 401 replay,
`invalid_grant`, 403 in development mode, 429 during pagination with resume, opt-in 429 retries
and a quota 429. Fixtures and their sources: [`PROVENANCE.md`](PROVENANCE.md). What Concord
cost: [`FRICTION.md`](FRICTION.md).

`tests/add_items.rs`: batch boundaries and malformed URIs before I/O, ordered JSON and optional
position, refresh retention across writes, no 401/429/403/5xx replay, session/transport timeouts
before the head and during the body, pre-dispatch deadlines, and lost/unreadable results.
`tests/add_items_consumer.rs`: public consumer selects an owned playlist, adds a track, receives
the new snapshot and reads the changed playlist. Maintenance findings: [`MAINTENANCE.md`](MAINTENANCE.md).

### Operations and paging (Concord Next)

The existing traversal uses checked numeric offsets and reads `next` only as a presence flag; it never follows that URL. Keeping this behavior preserves offset resume keys and the response-offset checks, including providers returning a next URL with a different spelling.
