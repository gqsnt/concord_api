//! Both connected workflows end to end against local fixtures, with the slice's required
//! failures: a refresh response without a replacement refresh token, and a 403 that may mean
//! development mode. Nothing here contacts Spotify.

mod common;

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use common::{BASIC, ManualClock, api, at, form, granted, json, resumed};
use concord_sdk_spotify_web_api::concord_pagination::{PagesError, Resume};
use concord_sdk_spotify_web_api::{
    ApiClass, AuthError, Error, Market, Playable, SessionOptions, pages,
};
use concord_testserver::{Gate, Recorded, Reply, Script, TestServer};

/// The provider's token endpoint and the Web API routes the workflows use.
fn provider(refreshes: Script) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |request: &Recorded| match (request.method.as_str(), request.path()) {
        ("POST", "/api/token") => match form(request, "grant_type").as_deref() {
            Some("authorization_code") => json(200, fixture!("token-grant.json")),
            Some("refresh_token") => refreshes.next(),
            _ => json(400, fixture!("oauth-invalid-grant.json")),
        },
        ("GET", "/v1/search") => match request.query_all("offset").first().map(String::as_str) {
            None | Some("0") => json(200, fixture!("search-artists-1.json")),
            Some("2") => json(200, fixture!("search-artists-2.json")),
            _ => json(400, fixture!("error-400.json")),
        },
        ("GET", "/v1/artists/0SyntheticArtist000003") => json(200, fixture!("artist.json")),
        ("GET", "/v1/me/playlists") => {
            match request.query_all("offset").first().map(String::as_str) {
                None | Some("0") => json(200, fixture!("me-playlists-1.json")),
                Some("2") => json(200, fixture!("me-playlists-2.json")),
                _ => json(400, fixture!("error-400.json")),
            }
        }
        ("GET", "/v1/playlists/0SyntheticPlaylist0003") => json(200, fixture!("playlist.json")),
        ("GET", "/v1/playlists/0SyntheticPlaylist0003/tracks") => {
            json(200, fixture!("playlist-items-2.json"))
        }
        _ => json(404, fixture!("error-404.json")),
    }
}

fn bearer(request: &Recorded) -> Option<&str> {
    request.header("authorization")
}

#[tokio::test]
async fn search_artists_then_retrieve_the_selected_artist() {
    let server = TestServer::start(provider(Script::new([]))).await;
    let clock = ManualClock::new();
    let (_oauth, grant, session) = granted(&server, &clock).await;
    assert_eq!(grant.expires_in(), Duration::from_secs(3600));
    let api = api(&server, &session);

    // Walk every page of the search (two pages of two), then pick by name.
    let artists = pages(api.search_artists("Nina").set_limit(2))
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    let names: Vec<&str> = artists.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Nina Synthetic", "The Ninas", "Nina Simone"]);
    let selected = artists.iter().find(|a| a.name == "Nina Simone").unwrap();
    // The search result omits optional fields; the SDK does not require them.
    assert_eq!(selected.popularity, None);
    assert!(selected.images.is_empty());

    let artist = api.artist(selected.id.clone()).send().await.unwrap();
    assert_eq!(artist.id, selected.id);
    assert_eq!(artist.genres, ["synthetic jazz", "synthetic soul"]);
    assert_eq!(artist.followers.unwrap().total, 4_100_000);
    assert_eq!(artist.popularity, Some(70));

    let requests = server.requests();
    assert_eq!(
        at(&requests, "/api/token").len(),
        1,
        "the grant's access token is reused"
    );
    let searches = at(&requests, "/v1/search");
    assert_eq!(searches[0].target, "/v1/search?q=Nina&limit=2&type=artist");
    assert_eq!(
        searches[1].target,
        "/v1/search?q=Nina&limit=2&offset=2&type=artist"
    );
    let fetched = at(&requests, "/v1/artists/0SyntheticArtist000003");
    assert_eq!(fetched.len(), 1);
    for request in searches.iter().chain(&fetched) {
        assert_eq!(request.method, "GET");
        assert_eq!(bearer(request), Some("Bearer synthetic-access-1"));
        assert_eq!(request.header("accept"), Some("application/json"));
    }
}

#[tokio::test]
async fn list_user_playlists_then_fetch_the_selected_playlist_across_refreshes() {
    // First refresh: no replacement refresh token. Second refresh: rotated.
    let refreshes = Script::new([
        json(200, fixture!("token-refresh-kept.json")),
        json(200, fixture!("token-refresh-rotated.json")),
    ]);
    let server = TestServer::start(provider(refreshes.clone())).await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);

    // Page 1 with the grant's access token; then the token expires before page 2.
    let mut listing = pages(api.me().playlists().set_limit(2));
    let first = listing.next_page().await.unwrap().unwrap();
    assert_eq!(first.items.len(), 2);
    clock.advance(Duration::from_secs(3600));
    let second = listing.next_page().await.unwrap().unwrap();
    assert!(
        listing.next_page().await.unwrap().is_none(),
        "the provider's last page was read"
    );
    let selected = second.items.iter().find(|p| p.name == "Road trip").unwrap();
    assert_eq!(selected.public, None);
    assert_eq!(selected.owner.id, "synthetic.user");

    // The refresh response carried no refresh token: the stored one is still the original.
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-1"
    );

    let playlist = api
        .playlist(selected.id.clone())
        .get()
        .set_market(Market::from_token())
        .send()
        .await
        .unwrap();
    assert_eq!(playlist.id, selected.id);
    assert_eq!(playlist.tracks.items.len(), 4);
    let next = playlist.tracks.next_offset().unwrap();
    assert_eq!(next, Some(4));

    // Expire again, then read the rest of the playlist's items.
    clock.advance(Duration::from_secs(3600));
    let rest = pages(
        api.playlist(playlist.id.clone())
            .items()
            .set_limit(playlist.tracks.limit),
    )
    .starting_at(Resume::At(next.unwrap()))
    .items()
    .collect_complete()
    .await
    .unwrap();
    let all: Vec<_> = playlist.tracks.items.iter().chain(&rest).collect();
    assert_eq!(all.len(), playlist.tracks.total as usize);
    assert!(matches!(&all[0].track, Some(Playable::Track(t)) if t.name == "Synthetic Song One"));
    assert!(matches!(&all[1].track, Some(Playable::Episode(e)) if e.name == "Synthetic Episode"));
    assert!(all[2].track.is_none(), "an unavailable item is null");
    assert!(matches!(&all[3].track, Some(Playable::Track(t)) if t.id.is_none() && t.is_local));
    assert!(matches!(&all[5].track, Some(Playable::Track(t)) if t.name == "Synthetic Song Six"));
    // The second refresh rotated the refresh token.
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-2"
    );
    assert_eq!(refreshes.remaining(), 0);

    let requests = server.requests();
    let tokens = at(&requests, "/api/token");
    assert_eq!(tokens.len(), 3, "one exchange, two refreshes");
    for refresh in &tokens[1..] {
        assert_eq!(refresh.header("authorization"), Some(BASIC));
        assert_eq!(
            form(refresh, "grant_type").as_deref(),
            Some("refresh_token")
        );
        // Both refreshes sent the original token: the first response did not erase it.
        assert_eq!(
            form(refresh, "refresh_token").as_deref(),
            Some("synthetic-refresh-1")
        );
    }
    let lists = at(&requests, "/v1/me/playlists");
    assert_eq!(lists[0].target, "/v1/me/playlists?limit=2");
    assert_eq!(bearer(lists[0]), Some("Bearer synthetic-access-1"));
    assert_eq!(lists[1].target, "/v1/me/playlists?limit=2&offset=2");
    assert_eq!(bearer(lists[1]), Some("Bearer synthetic-access-kept"));
    let fetched = at(&requests, "/v1/playlists/0SyntheticPlaylist0003");
    assert_eq!(
        fetched[0].target,
        "/v1/playlists/0SyntheticPlaylist0003?market=from_token"
    );
    assert_eq!(bearer(fetched[0]), Some("Bearer synthetic-access-kept"));
    let items = at(&requests, "/v1/playlists/0SyntheticPlaylist0003/tracks");
    assert_eq!(
        items[0].target,
        "/v1/playlists/0SyntheticPlaylist0003/tracks?limit=4&offset=4"
    );
    assert_eq!(bearer(items[0]), Some("Bearer synthetic-access-rotated"));
}

#[tokio::test]
async fn a_missing_replacement_after_a_401_keeps_the_refresh_token_for_the_next_refresh() {
    // The provider refuses the first access token (401); the refresh answers without a
    // replacement; a later refresh must still send the original refresh token.
    let refreshes = Script::new([
        json(200, fixture!("token-refresh-kept.json")),
        json(200, fixture!("token-refresh-kept.json")),
    ]);
    let artist_calls = Arc::new(AtomicUsize::new(0));
    let calls = artist_calls.clone();
    let routes = provider(refreshes.clone());
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == "/v1/artists/0SyntheticArtist000003"
            && calls.fetch_add(1, Ordering::SeqCst) == 0
        {
            return json(401, fixture!("error-401.json"));
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let session = resumed(&server, "synthetic-refresh-stored", &clock);
    let api = api(&server, &session);
    let id = concord_sdk_spotify_web_api::SpotifyId::new("0SyntheticArtist000003").unwrap();

    // Resumed: refresh #1, then the artist answers 401, refresh #2, replay succeeds.
    let artist = api.artist(id.clone()).send().await.unwrap();
    assert_eq!(artist.name, "Nina Simone");
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-stored"
    );

    let requests = server.requests();
    let tokens = at(&requests, "/api/token");
    assert_eq!(tokens.len(), 2);
    for refresh in &tokens {
        assert_eq!(
            form(refresh, "refresh_token").as_deref(),
            Some("synthetic-refresh-stored")
        );
    }
    let attempts = at(&requests, "/v1/artists/0SyntheticArtist000003");
    assert_eq!(attempts.len(), 2, "one 401, one replay");
    assert!(
        attempts
            .iter()
            .all(|r| bearer(r) == Some("Bearer synthetic-access-kept"))
    );
}

#[tokio::test]
async fn a_403_may_mean_development_mode_and_never_refreshes() {
    let routes = provider(Script::new([]));
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == "/v1/me/playlists" {
            return json(403, fixture!("error-403.json"));
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);

    let error = api.me().playlists().send().await.unwrap_err();
    assert_eq!(error.api_class(), Some(ApiClass::Forbidden));
    assert!(error.may_be_development_mode());
    let failure = error.api().unwrap();
    assert_eq!(failure.status(), 403);
    assert_eq!(failure.document().unwrap().message, "Forbidden");
    assert!(error.to_string().contains("development-mode"));

    // The same pagination walk ends with the provider error at the first page.
    let walk = pages(api.me().playlists())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(walk.error.provider_error(), Some(e) if e.may_be_development_mode()));
    assert_eq!(walk.error.resume(), Some(&Resume::Start));

    // The access token was neither refreshed nor invalidated: the search reuses it.
    let found = api.search_artists("Nina").send().await.unwrap();
    assert_eq!(found.artists.items.len(), 2);
    let requests = server.requests();
    assert_eq!(
        at(&requests, "/api/token").len(),
        1,
        "the code exchange only"
    );
    assert_eq!(
        at(&requests, "/v1/me/playlists").len(),
        2,
        "no replay of a 403"
    );
    assert!(
        requests
            .iter()
            .filter(|r| r.path().starts_with("/v1/"))
            .all(|r| bearer(r) == Some("Bearer synthetic-access-1"))
    );
}

#[tokio::test]
async fn concurrent_requests_share_one_refresh() {
    let gate = Gate::new();
    let held = gate.clone();
    let routes = provider(Script::new([]));
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == "/api/token" {
            return json(200, fixture!("token-refresh-rotated.json")).after(held.clone());
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let session = resumed(&server, "synthetic-refresh-stored", &clock);
    let api = api(&server, &session);

    let first = tokio::spawn({
        let api = api.clone();
        async move { api.search_artists("Nina").send().await }
    });
    // The first request's refresh is on the wire and held.
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| !at(&s.requests(), "/api/token")
                .is_empty())
            .await
    );
    let second = tokio::spawn({
        let api = api.clone();
        async move { api.me().playlists().send().await }
    });
    // A second refresh would be recorded (the server records before holding the reply).
    assert!(
        !server
            .wait_until(Duration::from_millis(300), |s| at(
                &s.requests(),
                "/api/token"
            )
            .len()
                > 1)
            .await
    );
    assert!(
        at(&server.requests(), "/v1/me/playlists").is_empty(),
        "the second call waits"
    );
    gate.open();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();

    let requests = server.requests();
    assert_eq!(at(&requests, "/api/token").len(), 1, "one shared refresh");
    assert_eq!(
        bearer(at(&requests, "/v1/search")[0]),
        Some("Bearer synthetic-access-rotated")
    );
    assert_eq!(
        bearer(at(&requests, "/v1/me/playlists")[0]),
        Some("Bearer synthetic-access-rotated")
    );
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-2"
    );
}

#[tokio::test]
async fn concurrent_401s_on_the_same_token_share_one_refresh() {
    // Both requests use the grant's token and both are refused; the refresh is held until both
    // 401s are on the wire. A delayed 401 must not trigger a second refresh.
    let gate = Gate::new();
    let held = gate.clone();
    let routes = provider(Script::new([]));
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == "/api/token"
            && form(request, "grant_type").as_deref() == Some("refresh_token")
        {
            return json(200, fixture!("token-refresh-rotated.json")).after(held.clone());
        }
        if request.path().starts_with("/v1/")
            && bearer(request) == Some("Bearer synthetic-access-1")
        {
            return json(401, fixture!("error-401.json"));
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);

    let search = tokio::spawn({
        let api = api.clone();
        async move { api.search_artists("Nina").send().await }
    });
    let playlists = tokio::spawn({
        let api = api.clone();
        async move { api.me().playlists().send().await }
    });
    let both_refused = |s: &TestServer| {
        let requests = s.requests();
        at(&requests, "/v1/search").len() == 1
            && at(&requests, "/v1/me/playlists").len() == 1
            && at(&requests, "/api/token").len() == 2
    };
    assert!(
        server
            .wait_until(Duration::from_secs(5), both_refused)
            .await
    );
    assert!(
        !server
            .wait_until(Duration::from_millis(300), |s| at(
                &s.requests(),
                "/api/token"
            )
            .len()
                > 2)
            .await,
        "a second refresh started"
    );
    gate.open();
    search.await.unwrap().unwrap();
    playlists.await.unwrap().unwrap();

    let requests = server.requests();
    assert_eq!(
        at(&requests, "/api/token").len(),
        2,
        "the exchange and one refresh"
    );
    for path in ["/v1/search", "/v1/me/playlists"] {
        let attempts = at(&requests, path);
        assert_eq!(attempts.len(), 2, "{path}: one 401, one replay");
        assert_eq!(bearer(attempts[1]), Some("Bearer synthetic-access-rotated"));
    }
}

#[tokio::test]
async fn a_second_401_is_final_after_one_refresh() {
    let routes = provider(Script::new([json(
        200,
        fixture!("token-refresh-kept.json"),
    )]));
    let server = TestServer::start(move |request: &Recorded| {
        if request.path().starts_with("/v1/artists/") {
            return json(401, fixture!("error-401.json"));
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let (oauth, grant, _session) = granted(&server, &clock).await;
    // Even with an attempt budget left for 429 retries, a 401 is replayed once only.
    let session = oauth.session(grant, common::options(&clock).rate_limit_retries(3));
    let api = api(&server, &session);
    let id = concord_sdk_spotify_web_api::SpotifyId::new("0SyntheticArtist000003").unwrap();

    let error = api.artist(id).send().await.unwrap_err();
    assert_eq!(error.api_class(), Some(ApiClass::Unauthorized));
    assert_eq!(
        error.api().unwrap().document().unwrap().message,
        "The access token expired"
    );
    let requests = server.requests();
    assert_eq!(
        at(&requests, "/api/token").len(),
        2,
        "exchange, then exactly one refresh"
    );
    let attempts = at(&requests, "/v1/artists/0SyntheticArtist000003");
    assert_eq!(bearer(attempts[0]), Some("Bearer synthetic-access-1"));
    assert_eq!(bearer(attempts[1]), Some("Bearer synthetic-access-kept"));
    assert_eq!(attempts.len(), 2);
}

#[tokio::test]
async fn invalid_grant_stops_refreshing_until_the_user_authorizes_again() {
    let refreshes = Script::new([json(400, fixture!("oauth-invalid-grant.json"))]);
    let server = TestServer::start(provider(refreshes.clone())).await;
    let clock = ManualClock::new();
    let session = resumed(&server, "synthetic-refresh-revoked", &clock);
    let api = api(&server, &session);

    let error = api.me().playlists().send().await.unwrap_err();
    let auth = error.auth().unwrap();
    assert!(matches!(auth, AuthError::InvalidGrant(f) if f.code() == Some("invalid_grant")));
    assert!(auth.requires_reauthorization());
    assert!(
        session.refresh_token().is_none(),
        "a rejected refresh token is not offered"
    );

    // The provider says not to retry the refresh: nothing is sent this time.
    let again = api.me().playlists().send().await.unwrap_err();
    assert!(matches!(
        again.auth(),
        Some(AuthError::ReauthorizationRequired)
    ));
    assert_eq!(at(&server.requests(), "/api/token").len(), 1);
    assert!(at(&server.requests(), "/v1/me/playlists").is_empty());

    // After a new authorization, the same session (and client) works again.
    let oauth = common::oauth(&server);
    let grant = oauth
        .exchange_code(
            &concord_sdk_spotify_web_api::AuthorizationCode::new("synthetic-code-2"),
            common::REDIRECT,
        )
        .await
        .unwrap();
    session.replace_grant(grant);
    let playlists = api.me().playlists().send().await.unwrap();
    assert_eq!(playlists.items.len(), 2);
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-1"
    );
}

#[tokio::test]
async fn a_429_during_pagination_resumes_from_the_failed_page() {
    let throttled = Arc::new(AtomicUsize::new(0));
    let count = throttled.clone();
    let routes = provider(Script::new([]));
    let server = TestServer::start(move |request: &Recorded| {
        if request.path() == "/v1/me/playlists"
            && request.query_all("offset") == ["2"]
            && count.fetch_add(1, Ordering::SeqCst) == 0
        {
            return json(429, fixture!("error-429-rate.json")).header("retry-after", "7");
        }
        routes(request)
    })
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);

    // Default options: a 429 is returned with its delay, not retried.
    let listing = api.me().playlists().set_limit(2);
    let incomplete = pages(listing.clone())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2, "page 1 is kept");
    let resume = incomplete.error.resume().cloned().unwrap();
    assert_eq!(resume, Resume::At(2));
    let PagesError::Provider { error, .. } = incomplete.error else {
        panic!("a provider failure");
    };
    assert_eq!(error.api_class(), Some(ApiClass::RateLimited));
    assert_eq!(error.retry_after(), Some(Duration::from_secs(7)));

    // The caller waits as told (not here), then resumes at the failed page.
    let rest = pages(listing)
        .starting_at(resume)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].name, "Road trip");
    assert_eq!(at(&server.requests(), "/v1/me/playlists").len(), 3);
}

#[tokio::test]
async fn opted_in_429_retries_wait_retry_after_and_never_retry_a_quota() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let routes = provider(Script::new([]));
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        "/v1/search" if count.fetch_add(1, Ordering::SeqCst) == 0 => {
            json(429, fixture!("error-429-rate.json")).header("retry-after", "1")
        }
        "/v1/me/playlists" => {
            json(429, fixture!("error-429-quota.json")).header("retry-after", "0")
        }
        _ => routes(request),
    })
    .await;
    let clock = ManualClock::new();
    let oauth = common::oauth(&server);
    let grant = oauth
        .exchange_code(
            &concord_sdk_spotify_web_api::AuthorizationCode::new("synthetic-code"),
            common::REDIRECT,
        )
        .await
        .unwrap();
    let clock_now = clock.clone();
    let session = oauth.session(
        grant,
        SessionOptions::new()
            .clock(move || clock_now.now())
            .rate_limit_retries(2)
            .max_retry_wait(Duration::from_secs(2)),
    );
    let api = api(&server, &session);

    let started = std::time::Instant::now();
    let found = api.search_artists("Nina").send().await.unwrap();
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "Retry-After is a minimum"
    );
    assert_eq!(found.artists.total, 3);
    assert_eq!(at(&server.requests(), "/v1/search").len(), 2);

    let quota = api.me().playlists().send().await.unwrap_err();
    assert_eq!(quota.api_class(), Some(ApiClass::QuotaExceeded));
    assert_eq!(
        quota.api().unwrap().document().unwrap().reason.as_deref(),
        Some("QUOTA_EXCEEDED")
    );
    assert_eq!(
        at(&server.requests(), "/v1/me/playlists").len(),
        1,
        "a quota is not retried"
    );
    assert!(matches!(quota, Error::Api(_)));
}
