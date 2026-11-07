//! Fresh consumer application: public SDK APIs, synthetic loopback fixtures only.

use std::{
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use concord_sdk_spotify_web_api::{
    ApiClass, AppCredentials, AuthError, AuthorizationCode, AuthorizeRequest, ClientSecret, Error,
    Market, OAuth, Playable, RefreshToken, Session, SessionOptions, SpotifyId, State, WebApi,
    WebApiConfig,
    concord::{self, ErrorKind},
    concord_pagination::{Outcome, Resume},
    pages, pages_from,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde_json::{Value, json};

const ARTIST: &str = "7G1GBhoKtEPnP86X2PvEYO";
const OTHER_ARTIST: &str = "2w9zwq3AktTeYYMuhMjju8";
const STORED_REFRESH: &str = "fixture-stored-refresh-secret";
const SCOPES: &str = "playlist-read-private playlist-read-collaborative";
const BASIC: &str = "Basic Y29uc3VtZXI6Zml4dHVyZS1zZWNyZXQ=";

fn document(status: u16, value: Value) -> Reply {
    Reply::json(status, value.to_string())
}

fn artist(id: &str, name: &str) -> Value {
    json!({"id": id, "name": name, "uri": format!("spotify:artist:{id}"),
        "external_urls": {"spotify": format!("https://open.spotify.com/artist/{id}")}})
}

fn page(path: &str, items: Vec<Value>, offset: u32, limit: u32, total: u32, more: bool) -> Value {
    json!({"href": format!("https://api.spotify.com/v1/{path}?offset={offset}&limit={limit}"),
        "items": items, "offset": offset, "limit": limit, "total": total,
        "next": more.then(|| format!("https://api.spotify.com/v1/{path}?offset={}&limit={limit}", offset + limit)),
        "previous": (offset > 0).then(|| format!("https://api.spotify.com/v1/{path}?offset=0&limit={limit}"))})
}

fn clients(server: &TestServer) -> (concord::reqwest::Client, OAuth) {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let app = AppCredentials::new("consumer", &ClientSecret::new("fixture-secret")).unwrap();
    let oauth = OAuth::new(http.clone(), app)
        .with_base_url(server.url())
        .unwrap();
    (http, oauth)
}

fn web_api(server: &TestServer, http: concord::reqwest::Client, session: &Session) -> WebApi {
    WebApi::new(
        http,
        WebApiConfig {
            session: session.clone(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

fn assert_query(request: &Recorded, expected: &[(&str, &str)]) {
    let mut actual = request.query_pairs();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn assert_token_form(request: &Recorded, expected: &[(&str, &str)]) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/api/token");
    assert_eq!(request.header("authorization"), Some(BASIC));
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    let mut actual = request.form_pairs();
    let mut expected: Vec<_> = expected
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn assert_get(request: &Recorded, bearer: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.header("authorization"), Some(bearer));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.body_len, 0);
    assert!(request.body.is_empty());
}

fn assert_redacted(formatted: &str, extra: &[&str]) {
    for secret in [
        "fixture-secret",
        STORED_REFRESH,
        "fixture-access-A1",
        "fixture-access-A2",
        BASIC,
    ]
    .into_iter()
    .chain(extra.iter().copied())
    {
        assert!(
            !formatted.contains(secret),
            "formatted diagnostic exposed a fixture secret"
        );
    }
}

#[tokio::test]
async fn search_select_and_retrieve_artist() {
    let refreshes = AtomicUsize::new(0);
    let second_pages = AtomicUsize::new(0);
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        match request.path() {
            "/api/token" => {
                let n = refreshes.fetch_add(1, Ordering::SeqCst) + 1;
                // Deliberately no replacement refresh_token, on either refresh.
                let mut token = json!({"access_token": format!("fixture-access-A{n}"),
                    "token_type": "Bearer", "expires_in": 3600});
                if n == 1 { token["scope"] = json!(SCOPES); }
                document(200, token)
            }
            "/v1/search" => {
                let q = request.query_all("q");
                if q == ["bad-json"] {
                    return Reply::json(200, "{\"artists\":").header("x-request-id", "decode-head-secret");
                }
                if q == ["wrong-media"] { return Reply::text(200, "text/html", "<html>fixture</html>"); }
                if q == ["bad-offset"] {
                    return document(200, json!({"artists": page("search", vec![], 99, 1, 0, false)}));
                }
                let offset = request.query_all("offset");
                if offset == ["1"] {
                    if second_pages.fetch_add(1, Ordering::SeqCst) == 0 {
                        return document(429, json!({"error": {"status": 429,
                            "message": "provider-message-secret"}}))
                            .header("Retry-After", "7").header("x-request-id", "rate-head-secret");
                    }
                    document(200, json!({"artists": page("search", vec![artist(OTHER_ARTIST, "Nina Simone Tribute")], 1, 1, 2, false)}))
                } else {
                    document(200, json!({"artists": page("search", vec![artist(ARTIST, "Nina Simone")], 0, 1, 2, true)}))
                }
            }
            path if path == format!("/v1/artists/{ARTIST}") => {
                if request.header("authorization") == Some("Bearer fixture-access-A1") {
                    document(401, json!({"error": {"status": 401, "message": "Expired access token"}}))
                } else {
                    let mut found = artist(ARTIST, "Nina Simone");
                    found["genres"] = json!(["vocal jazz", "soul"]);
                    document(200, found)
                }
            }
            "/v1/artists/missing" => document(404, json!({"error": {"status": 404, "message": "missing-artist-secret"}})),
            _ => document(500, json!({"error": {"status": 500, "message": "Unexpected fixture request"}})),
        }
    }).await;
    let (http, oauth) = clients(&server);
    let session = oauth.resume(RefreshToken::new(STORED_REFRESH), SessionOptions::new());
    let api = web_api(&server, http, &session);
    let search = api
        .search_artists("Nina Simone")
        .set_limit(1)
        .set_offset(0)
        .set_market(Market::country("FR").unwrap());

    // First working call: ordinary generated send, including the resumed session's refresh.
    let first = search.clone().send().await.unwrap_or_else(|error| {
        let paths: Vec<_> = server
            .requests()
            .into_iter()
            .map(|r| (r.method, r.target))
            .collect();
        panic!("first search: {error:?}; fixture targets: {paths:?}");
    });
    assert_eq!(first.artists.offset, 0);
    assert_eq!(first.artists.limit, 1);
    assert_eq!(first.artists.total, 2);
    assert_eq!(first.artists.next_offset().unwrap(), Some(1));
    assert_eq!(first.artists.items[0].name, "Nina Simone");
    assert!(first.artists.items[0].popularity.is_none());
    assert!(first.artists.items[0].followers.is_none());
    assert!(first.artists.items[0].genres.is_empty());
    assert!(first.artists.items[0].images.is_empty());
    assert_eq!(session.refresh_token().unwrap().expose(), STORED_REFRESH);

    let incomplete = pages(search.clone())
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    let mut artists = incomplete.collected;
    let (error, failed, progress) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At(1));
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (2, 1, 1)
    );
    assert_eq!(error.api_class(), Some(ApiClass::RateLimited));
    assert_eq!(error.retry_after(), Some(Duration::from_secs(7)));
    let failure = error.api().unwrap();
    assert_eq!(failure.status(), 429);
    assert_eq!(failure.meta().status().as_u16(), 429);
    assert_eq!(failure.meta().headers()["x-request-id"], "rate-head-secret");
    assert_eq!(
        failure.meta().operation().unwrap().name(),
        "WebApi.search_artists"
    );
    assert_eq!(
        failure.document().unwrap().message,
        "provider-message-secret"
    );
    assert_redacted(
        &format!("{error} {error:?} {:?}", failure.meta()),
        &["provider-message-secret", "rate-head-secret"],
    );

    let before_resume = server.requests().len();
    let rest = pages_from(search.clone(), failed)
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        server.requests().len(),
        before_resume + 1,
        "terminal page needs no extra probe"
    );
    artists.extend(rest);
    assert_eq!(
        artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        ["Nina Simone", "Nina Simone Tribute"]
    );

    let selected = api.artist(artists[0].id.clone()).send().await.unwrap();
    assert_eq!(selected.id, artists[0].id);
    assert_eq!(selected.genres, ["vocal jazz", "soul"]);
    assert_eq!(session.refresh_token().unwrap().expose(), STORED_REFRESH);
    assert_eq!(session.scope().as_deref(), Some(SCOPES));
    assert_redacted(&format!("{session:?} {oauth:?} {api:?}"), &[]);

    let capped = pages(search.clone())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(capped.collected.len(), 1);
    assert_eq!(capped.error.outcome(), Outcome::Bounded);
    assert_eq!(capped.error.resume(), Some(&Resume::At(1)));

    for (query, kind) in [
        ("bad-json", ErrorKind::Decode),
        ("wrong-media", ErrorKind::Metadata),
    ] {
        let error = api.search_artists(query).send().await.unwrap_err();
        let cause = error.concord().unwrap();
        assert_eq!(cause.kind(), kind);
        assert_eq!(cause.head().unwrap().status().as_u16(), 200);
        assert_eq!(cause.endpoint(), Some("WebApi.search_artists"));
        if kind == ErrorKind::Decode {
            assert_eq!(
                cause.head_meta().unwrap().headers()["x-request-id"],
                "decode-head-secret"
            );
        }
        assert_redacted(&format!("{error} {error:?}"), &["decode-head-secret"]);
    }
    // Observed contract gap: initial pages() accepts the wrong offset even when set explicitly.
    let accepted = pages(api.search_artists("bad-offset").set_offset(0))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(accepted.is_empty());
    // An explicit resume does validate the same response's offset.
    let bad_page = pages_from(api.search_artists("bad-offset"), Resume::At(0))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(bad_page.collected.is_empty());
    assert!(matches!(
        bad_page.error.provider_error(),
        Some(Error::Protocol(_))
    ));
    let missing = api
        .artist(SpotifyId::new("missing").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(missing.api_class(), Some(ApiClass::NotFound));
    assert_eq!(missing.api().unwrap().document().unwrap().status, 404);
    assert_redacted(
        &format!("{missing} {missing:?}"),
        &["missing-artist-secret"],
    );
    let before_invalid = server.requests().len();
    assert!(SpotifyId::new(format!("spotify:artist:{ARTIST}")).is_err());
    assert_eq!(server.requests().len(), before_invalid);

    let requests = server.requests();
    assert!(requests.iter().all(|r| r.host == server.addr().to_string()));
    let tokens: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == "/api/token")
        .collect();
    assert_eq!(
        tokens.len(),
        2,
        "401 performs exactly one refresh and replay"
    );
    for token in tokens {
        assert_token_form(
            token,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", STORED_REFRESH),
            ],
        );
    }
    let searches: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == "/v1/search" && r.query_all("q") == ["Nina Simone"])
        .collect();
    assert_eq!(searches.len(), 5); // direct first call, prefix, failed page, resume, capped prefix
    for (request, offset) in searches.iter().zip(["0", "0", "1", "1", "0"]) {
        assert_query(
            request,
            &[
                ("q", "Nina Simone"),
                ("type", "artist"),
                ("market", "FR"),
                ("limit", "1"),
                ("offset", offset),
            ],
        );
    }
    let retrievals: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == format!("/v1/artists/{ARTIST}"))
        .collect();
    assert_eq!(retrievals.len(), 2);
    assert_get(retrievals[0], "Bearer fixture-access-A1");
    assert_get(retrievals[1], "Bearer fixture-access-A2");
    assert_query(retrievals[1], &[]);
    // Every GET uses the current token, JSON Accept and no body.
    let replay_at = requests
        .iter()
        .position(|r| r.path() == format!("/v1/artists/{ARTIST}"))
        .unwrap();
    for (index, request) in requests
        .iter()
        .enumerate()
        .filter(|(_, r)| r.method == "GET")
    {
        assert_get(
            request,
            if index <= replay_at {
                "Bearer fixture-access-A1"
            } else {
                "Bearer fixture-access-A2"
            },
        );
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

const PLAYLIST: &str = "37i9dQZF1DX4sWSpwq3LiO";
const OTHER_PLAYLIST: &str = "37i9dQZF1DWVqfgj8NZEp1";
const CODE_REFRESH: &str = "fixture-code-refresh-secret";
const CODE_ACCESS: &str = "fixture-code-access-secret";
const CALLBACK: &str = "https://consumer.example/callback";

fn playlist_summary(id: &str, name: &str) -> Value {
    json!({"id": id, "name": name, "description": "A listener's jazz collection",
        "collaborative": false, "owner": {"id": "listener-36", "display_name": "Camille"},
        "snapshot_id": "fixture-snapshot-v1", "uri": format!("spotify:playlist:{id}"),
        "tracks": {"href": format!("https://api.spotify.com/v1/playlists/{id}/tracks"), "total": 4}})
}

#[tokio::test]
async fn list_select_and_fetch_authorized_users_playlist() {
    let second_pages = AtomicUsize::new(0);
    let continuations = AtomicUsize::new(0);
    let server = TestServer::start(move |request: &Recorded| -> Reply {
        match request.path() {
            "/api/token" => {
                if request.form_pairs().iter().any(|(k, v)| k == "grant_type" && v == "authorization_code") {
                    document(200, json!({"access_token": CODE_ACCESS, "token_type": "Bearer",
                        "expires_in": 3600, "refresh_token": CODE_REFRESH, "scope": SCOPES}))
                } else {
                    document(400, json!({"error": "invalid_grant", "error_description": "revoked-description-secret"}))
                        .header("x-request-id", "oauth-head-secret")
                }
            }
            "/v1/me/playlists" => {
                if request.query_all("offset") == ["1"] {
                    if second_pages.fetch_add(1, Ordering::SeqCst) == 0 {
                        return document(403, json!({"error": {"status": 403,
                            "message": "allowlist-description-secret"}})).header("x-request-id", "forbidden-head-secret");
                    }
                    document(200, page("me/playlists", vec![playlist_summary(OTHER_PLAYLIST, "Evening jazz")], 1, 1, 2, false))
                } else {
                    document(200, page("me/playlists", vec![playlist_summary(PLAYLIST, "Nina and friends")], 0, 1, 2, true))
                }
            }
            path if path == format!("/v1/playlists/{PLAYLIST}") => {
                let mut playlist = playlist_summary(PLAYLIST, "Nina and friends");
                playlist["tracks"] = page(&format!("playlists/{PLAYLIST}/tracks"), vec![
                    json!({"added_at": "2026-10-29T18:30:00Z", "added_by": {"id": "listener-36"}, "is_local": false,
                        "track": {"type": "track", "id": "0F01Bl7xgRMIjJrzX0Ri8H", "name": "Feeling Good",
                            "duration_ms": 174000, "artists": [{"id": ARTIST, "name": "Nina Simone"}], "is_local": false}}),
                    json!({"added_at": "2026-10-30T18:30:00Z", "is_local": false,
                        "track": {"type": "episode", "id": "4rOoJ6Egrf8K2IrywzwOMk", "name": "A life in jazz", "duration_ms": 1800000}})
                ], 0, 2, 4, true);
                document(200, playlist)
            }
            path if path == format!("/v1/playlists/{PLAYLIST}/tracks") => {
                if continuations.fetch_add(1, Ordering::SeqCst) == 0 {
                    return document(429, json!({"error": {"status": 429,
                        "message": "quota-description-secret", "reason": "QUOTA_EXCEEDED"}}))
                        .header("Retry-After", "0").header("x-request-id", "quota-head-secret");
                }
                document(200, page(&format!("playlists/{PLAYLIST}/tracks"), vec![
                    json!({"added_at": null, "is_local": false, "track": null}),
                    json!({"added_at": "2026-10-31T18:30:00Z", "is_local": true,
                        "track": {"type": "track", "id": null, "name": "Home piano recording", "duration_ms": 125000,
                            "artists": [{"id": null, "name": "Camille"}], "is_local": true}})
                ], 2, 2, 4, false))
            }
            "/v1/playlists/unreadable" => Reply::text(403, "text/html", "<html>proxy-description-secret</html>")
                .header("x-request-id", "proxy-head-secret"),
            _ => document(500, json!({"error": {"status": 500, "message": "Unexpected fixture request"}})),
        }
    }).await;
    let (http, oauth) = clients(&server);
    let authorization = AuthorizeRequest {
        redirect_uri: CALLBACK.into(),
        scopes: SCOPES.split_whitespace().map(str::to_owned).collect(),
        state: State::new("csrf-state-secret"),
        show_dialog: true,
    };
    let authorize_url = oauth.authorize_url(&authorization).unwrap();
    assert_eq!(authorize_url.path(), "/authorize");
    assert_eq!(authorize_url.host_str(), Some("127.0.0.1"));
    let mut actual: Vec<_> = authorize_url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    actual.sort();
    let mut expected: Vec<_> = [
        ("response_type", "code"),
        ("client_id", "consumer"),
        ("redirect_uri", CALLBACK),
        ("scope", SCOPES),
        ("state", "csrf-state-secret"),
        ("show_dialog", "true"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect();
    expected.sort();
    assert_eq!(actual, expected);
    assert!(
        server.requests().is_empty(),
        "building the browser URL sends nothing"
    );

    // Simulated callback: state equality is the caller's responsibility.
    let returned_state = "csrf-state-secret";
    assert_eq!(returned_state, authorization.state.expose());
    let code = AuthorizationCode::new("fixture-authorization-code-secret");
    let grant = oauth.exchange_code(&code, CALLBACK).await.unwrap();
    assert_eq!(grant.expires_in(), Duration::from_secs(3600));
    assert_eq!(grant.scope(), Some(SCOPES));
    assert_eq!(grant.refresh_token().unwrap().expose(), CODE_REFRESH);
    assert_redacted(
        &format!("{code:?} {:?} {grant:?}", authorization.state),
        &[
            CODE_ACCESS,
            CODE_REFRESH,
            "fixture-authorization-code-secret",
            returned_state,
        ],
    );
    // Even opt-in retries must not repeat a forbidden response or quota 429.
    let session = oauth.session(grant, SessionOptions::new().rate_limit_retries(2));
    let api = web_api(&server, http.clone(), &session);
    let listing = api.me().playlists().set_limit(1).set_offset(0);
    let incomplete = pages(listing.clone())
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    let mut playlists = incomplete.collected;
    assert_eq!(playlists.len(), 1);
    assert!(
        playlists[0].public.is_none(),
        "development-mode omissions are representable"
    );
    let (error, failed, progress) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At(1));
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (2, 1, 1)
    );
    assert_eq!(error.api_class(), Some(ApiClass::Forbidden));
    assert!(error.may_be_development_mode());
    let failure = error.api().unwrap();
    assert_eq!(failure.status(), 403);
    assert_eq!(
        failure.meta().headers()["x-request-id"],
        "forbidden-head-secret"
    );
    assert_eq!(
        failure.meta().operation().unwrap().name(),
        "WebApi.me.playlists"
    );
    assert_eq!(
        failure.document().unwrap().message,
        "allowlist-description-secret"
    );
    assert_redacted(
        &format!("{error} {error:?} {:?}", failure.meta()),
        &[
            CODE_ACCESS,
            CODE_REFRESH,
            "allowlist-description-secret",
            "forbidden-head-secret",
        ],
    );
    assert_eq!(
        server.requests().len(),
        3,
        "403 neither refreshes nor retries"
    );
    assert_eq!(session.refresh_token().unwrap().expose(), CODE_REFRESH);

    // The fixture now allows access; retry is a deliberate caller action, using the same token.
    let rest = pages_from(listing, failed)
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    playlists.extend(rest);
    assert_eq!(
        playlists
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["Nina and friends", "Evening jazz"]
    );
    assert_eq!(
        server.requests().len(),
        4,
        "terminal listing page needs no probe"
    );

    let playlist = api
        .playlist(playlists[0].id.clone())
        .get()
        .set_market(Market::country("FR").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(playlist.id, playlists[0].id);
    assert_eq!(playlist.snapshot_id, "fixture-snapshot-v1");
    assert_eq!(playlist.owner.id, "listener-36");
    assert!(playlist.public.is_none());
    assert!(playlist.followers.is_none());
    assert_eq!(
        (
            playlist.tracks.offset,
            playlist.tracks.limit,
            playlist.tracks.total
        ),
        (0, 2, 4)
    );
    let offset = playlist.tracks.next_offset().unwrap().unwrap();
    assert_eq!(offset, 2);
    let items_request = api
        .playlist(playlist.id.clone())
        .items()
        .set_limit(playlist.tracks.limit)
        .set_market(Market::country("FR").unwrap());
    let mut entries = playlist.tracks.items;
    let incomplete = pages_from(items_request.clone(), Resume::At(offset))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert_eq!(
        entries.len(),
        2,
        "the embedded prefix survives continuation failure"
    );
    let (error, failed, _) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At(2));
    assert_eq!(error.api_class(), Some(ApiClass::QuotaExceeded));
    assert_eq!(error.retry_after(), Some(Duration::ZERO));
    assert!(!error.may_be_development_mode());
    let failure = error.api().unwrap();
    assert_eq!(
        failure.document().unwrap().reason.as_deref(),
        Some("QUOTA_EXCEEDED")
    );
    assert_eq!(failure.meta().status().as_u16(), 429);
    assert_eq!(
        failure.meta().headers()["x-request-id"],
        "quota-head-secret"
    );
    assert_redacted(
        &format!("{error} {error:?} {:?}", failure.meta()),
        &["quota-description-secret", "quota-head-secret"],
    );
    assert_eq!(
        server.requests().len(),
        6,
        "quota 429 is not retried even with a valid zero delay"
    );

    let before_resume = server.requests().len();
    entries.extend(
        pages_from(items_request, failed)
            .max_pages(NonZeroUsize::new(1).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap(),
    );
    assert_eq!(server.requests().len(), before_resume + 1);
    assert_eq!(entries.len(), 4);
    assert!(
        matches!(&entries[0].track, Some(Playable::Track(track)) if track.name == "Feeling Good" && track.id.is_some())
    );
    assert!(
        matches!(&entries[1].track, Some(Playable::Episode(episode)) if episode.name == "A life in jazz")
    );
    assert!(entries[2].track.is_none());
    assert!(entries[3].is_local);
    assert!(
        matches!(&entries[3].track, Some(Playable::Track(track)) if track.id.is_none() && track.is_local && track.artists[0].id.is_none())
    );

    let unreadable = api
        .playlist(SpotifyId::new("unreadable").unwrap())
        .get()
        .send()
        .await
        .unwrap_err();
    assert_eq!(unreadable.api_class(), Some(ApiClass::Forbidden));
    assert_eq!(unreadable.api().unwrap().status(), 403);
    assert_eq!(
        unreadable.api().unwrap().document().unwrap_err().kind(),
        ErrorKind::Metadata
    );
    assert_redacted(
        &format!("{unreadable} {unreadable:?}"),
        &["proxy-description-secret", "proxy-head-secret"],
    );

    let revoked = oauth.resume(
        RefreshToken::new("revoked-refresh-secret"),
        SessionOptions::new(),
    );
    let revoked_api = web_api(&server, http, &revoked);
    let rejected = revoked_api.me().playlists().send().await.unwrap_err();
    assert!(matches!(rejected.auth(), Some(AuthError::InvalidGrant(_))));
    let failure = rejected.auth().unwrap().failure().unwrap();
    assert_eq!(failure.status(), 400);
    assert_eq!(failure.code(), Some("invalid_grant"));
    assert_eq!(
        failure.meta().headers()["x-request-id"],
        "oauth-head-secret"
    );
    assert!(rejected.auth().unwrap().requires_reauthorization());
    assert!(revoked.refresh_token().is_none());
    let before_rejected = server.requests().len();
    let again = revoked_api.me().playlists().send().await.unwrap_err();
    assert!(matches!(
        again.auth(),
        Some(AuthError::ReauthorizationRequired)
    ));
    assert_eq!(
        server.requests().len(),
        before_rejected,
        "revoked refresh tokens are never sent twice"
    );
    assert_redacted(
        &format!("{rejected} {rejected:?} {again} {again:?} {session:?} {api:?}"),
        &[
            CODE_ACCESS,
            CODE_REFRESH,
            "revoked-refresh-secret",
            "revoked-description-secret",
            "oauth-head-secret",
        ],
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert!(requests.iter().all(|r| r.host == server.addr().to_string()));
    let tokens: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == "/api/token")
        .collect();
    assert_eq!(
        tokens.len(),
        2,
        "403 and quota failures caused no token request"
    );
    assert_token_form(
        tokens[0],
        &[
            ("grant_type", "authorization_code"),
            ("code", "fixture-authorization-code-secret"),
            ("redirect_uri", CALLBACK),
        ],
    );
    assert_token_form(
        tokens[1],
        &[
            ("grant_type", "refresh_token"),
            ("refresh_token", "revoked-refresh-secret"),
        ],
    );
    let listings: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == "/v1/me/playlists")
        .collect();
    assert_eq!(listings.len(), 3);
    for (request, offset) in listings.iter().zip(["0", "1", "1"]) {
        assert_query(request, &[("limit", "1"), ("offset", offset)]);
    }
    let selected: Vec<_> = requests
        .iter()
        .filter(|r| r.path() == format!("/v1/playlists/{PLAYLIST}"))
        .collect();
    assert_eq!(selected.len(), 1);
    assert_query(selected[0], &[("market", "FR")]);
    let continuations: Vec<_> = requests
        .iter()
        .filter(|r| r.path().ends_with("/tracks"))
        .collect();
    assert_eq!(continuations.len(), 2);
    for request in continuations {
        assert_query(
            request,
            &[("market", "FR"), ("limit", "2"), ("offset", "2")],
        );
    }
    for request in requests.iter().filter(|r| r.method == "GET") {
        assert_get(request, "Bearer fixture-code-access-secret");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}
