//! Playlist-write regression tests. All HTTP traffic is synthetic loopback traffic.

mod common;

use std::time::Duration;

use common::{BASIC, ManualClock, api, at, form, granted, json, oauth, options};
use concord_sdk_spotify_web_api::{
    AddItems, ApiClass, Error, RefreshToken, SessionOptions, SpotifyId, WebApi, WebApiConfig,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use serde_json::{Value, json as value};

const PLAYLIST: &str = "0SyntheticPlaylist0003";
const TRACK: &str = "spotify:track:4iV5W9uYEdYUVa79Axb7Rh";
const EPISODE: &str = "spotify:episode:512ojhOuo1ktJprKbVcKyQ";
const TARGET: &str = "/v1/playlists/0SyntheticPlaylist0003/tracks";

fn id() -> SpotifyId {
    SpotifyId::new(PLAYLIST).unwrap()
}

fn batch() -> AddItems {
    AddItems::new([TRACK]).unwrap()
}

#[tokio::test]
async fn ordered_json_batches_return_snapshots_and_keep_the_refresh_token() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/api/token" => json(200, fixture!("token-refresh-kept.json")),
            TARGET => json(201, fixture!("playlist-snapshot.json")),
            _ => json(404, fixture!("error-404.json")),
        }
    })
    .await;
    let clock = ManualClock::new();
    let session = oauth(&server).resume(RefreshToken::new("stored-refresh"), options(&clock));
    let api = api(&server, &session);
    let uris: Vec<_> = (0..100)
        .map(|i| if i % 2 == 0 { TRACK } else { EPISODE })
        .collect();
    let request = api
        .playlist(id())
        .add_items(AddItems::new(uris.clone()).unwrap().position(0));
    let prepared = request.clone().prepare().unwrap();
    assert_eq!(prepared.request().method(), "POST");
    assert!(!prepared.request().headers().contains_key("authorization"));
    assert!(server.requests().is_empty(), "prepare must not refresh");
    let first = request.send().await.unwrap();
    assert_eq!(first.snapshot_id, "synthetic-new-snapshot+/=");
    assert_eq!(session.refresh_token().unwrap().expose(), "stored-refresh");

    clock.advance(Duration::from_secs(3600));
    let second = api.playlist(id()).add_items(batch()).send().await.unwrap();
    assert_eq!(second.snapshot_id, first.snapshot_id);
    assert_eq!(session.refresh_token().unwrap().expose(), "stored-refresh");

    let requests = server.requests();
    let writes = at(&requests, TARGET);
    assert_eq!(writes.len(), 2);
    for request in &writes {
        assert_eq!(request.method, "POST");
        assert_eq!(
            request.target, TARGET,
            "URIs must only appear in the JSON body"
        );
        assert_eq!(
            request.header("authorization"),
            Some("Bearer synthetic-access-kept")
        );
        assert_eq!(request.header("accept"), Some("application/json"));
        assert_eq!(request.header("content-type"), Some("application/json"));
    }
    assert_eq!(
        serde_json::from_slice::<Value>(&writes[0].body).unwrap(),
        value!({"uris": uris, "position": 0})
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&writes[1].body).unwrap(),
        value!({"uris": [TRACK]})
    );
    let tokens = at(&requests, "/api/token");
    assert_eq!(tokens.len(), 2);
    for token in tokens {
        assert_eq!(token.header("authorization"), Some(BASIC));
        assert_eq!(form(token, "grant_type").as_deref(), Some("refresh_token"));
        assert_eq!(
            form(token, "refresh_token").as_deref(),
            Some("stored-refresh")
        );
    }
    assert!(requests.iter().all(|r| r.host == server.addr().to_string()));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn invalid_batches_are_refused_before_even_a_token_request() {
    let server =
        TestServer::start(|_: &Recorded| -> Reply { panic!("invalid input reached HTTP") }).await;
    let session = oauth(&server).resume(RefreshToken::new("stored-refresh"), SessionOptions::new());
    let api = api(&server, &session);
    let mut invalid = vec![vec![], vec![TRACK.to_owned(); 101]];
    for uri in [
        "",
        "not-a-uri",
        "spotify:track:",
        "spotify:track:short",
        "spotify:artist:4iV5W9uYEdYUVa79Axb7Rh",
        "spotify:local:artist:album:track:123",
        "https://open.spotify.com/track/4iV5W9uYEdYUVa79Axb7Rh",
        "spotify:track:4iV5W9uYEdYUVa79Axb7R_",
        "spotify:track:4iV5W9uYEdYUVa79Axb7Ré",
        "spotify:track:4iV5W9uYEdYUVa79Axb7Rh:extra",
        "spotify:track:4iV5W9uYEdYUVa79Axb7Rh?x=1",
        "spotify:track:4iV5W9uYEdYUVa79Axb7Rh\n",
        "spotify:track:4iV5W9uYEdYUVa79Axb7R%68",
    ] {
        invalid.push(vec![TRACK.to_owned(), uri.to_owned()]);
    }
    for uris in invalid {
        let error = match AddItems::new(uris) {
            Ok(body) => api.playlist(id()).add_items(body).send().await.unwrap_err(),
            Err(error) => error,
        };
        assert!(matches!(error, Error::InvalidInput(_)), "{error:?}");
    }
    assert!(server.requests().is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_write_401_invalidates_the_used_token_without_refresh_or_replay() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match request.path() {
            "/api/token"
                if form(request, "grant_type").as_deref() == Some("authorization_code") =>
            {
                json(200, fixture!("token-grant.json"))
            }
            "/api/token" => json(200, fixture!("token-refresh-kept.json")),
            TARGET => json(401, fixture!("error-401.json")),
            "/v1/playlists/0SyntheticPlaylist0003" => json(200, fixture!("playlist.json")),
            _ => json(404, fixture!("error-404.json")),
        }
    })
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);
    let error = api
        .playlist(id())
        .add_items(batch())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.api_class(), Some(ApiClass::Unauthorized));
    assert_eq!(
        error.api().unwrap().meta().operation().unwrap().name(),
        "WebApi.playlist.add_items"
    );
    let before_read = server.requests();
    assert_eq!(at(&before_read, TARGET).len(), 1);
    assert_eq!(
        at(&before_read, "/api/token").len(),
        1,
        "only the initial code exchange"
    );
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-1"
    );
    // A separate caller-initiated read uses the invalidated session normally.
    api.playlist(id()).get().send().await.unwrap();
    let requests = server.requests();
    assert_eq!(at(&requests, TARGET).len(), 1);
    assert_eq!(at(&requests, "/api/token").len(), 2);
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-1"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn write_refusals_never_retry_even_with_rate_limit_retries_enabled() {
    for (status, body, class) in [
        (403, fixture!("error-403.json"), ApiClass::Forbidden),
        (429, fixture!("error-429-rate.json"), ApiClass::RateLimited),
        (
            429,
            fixture!("error-429-quota.json"),
            ApiClass::QuotaExceeded,
        ),
        (500, fixture!("error-500.json"), ApiClass::Server),
    ] {
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            match request.path() {
                "/api/token" => json(200, fixture!("token-grant.json")),
                TARGET => json(status, body).header("Retry-After", "0"),
                "/v1/playlists/0SyntheticPlaylist0003" => json(200, fixture!("playlist.json")),
                _ => json(404, fixture!("error-404.json")),
            }
        })
        .await;
        let oauth = oauth(&server);
        let grant = oauth
            .exchange_code(
                &concord_sdk_spotify_web_api::AuthorizationCode::new("code"),
                common::REDIRECT,
            )
            .await
            .unwrap();
        let session = oauth.session(grant, SessionOptions::new().rate_limit_retries(5));
        let api = api(&server, &session);
        let error = api
            .playlist(id())
            .add_items(batch())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.api_class(), Some(class));
        assert_eq!(error.may_be_development_mode(), status == 403);
        if status == 429 {
            assert_eq!(error.retry_after(), Some(Duration::ZERO));
        }
        api.playlist(id()).get().send().await.unwrap();
        let requests = server.requests();
        assert_eq!(at(&requests, TARGET).len(), 1);
        assert_eq!(
            at(&requests, "/api/token").len(),
            1,
            "a refusal must not invalidate the token"
        );
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn timeouts_before_the_head_or_during_the_body_are_uncertain_and_never_replayed() {
    // Exercise both the session budget and the user's HTTP timeout, before/after the 201 head.
    for session_deadline in [true, false] {
        for stalled_body in [false, true] {
            let server = TestServer::start(move |request: &Recorded| -> Reply {
                match request.path() {
                    "/api/token" => json(200, fixture!("token-grant.json")),
                    TARGET if stalled_body => json(201, "").with_body(Body::Stall),
                    TARGET => {
                        json(201, fixture!("playlist-snapshot.json")).delay(Duration::from_secs(3))
                    }
                    "/v1/playlists/0SyntheticPlaylist0003" => {
                        // Synthetic provider state: the POST was applied despite losing its reply.
                        let mut playlist: Value =
                            serde_json::from_str(fixture!("playlist.json")).unwrap();
                        playlist["snapshot_id"] = value!("synthetic-new-snapshot+/=");
                        playlist["tracks"]["items"][0]["track"]["id"] =
                            value!("4iV5W9uYEdYUVa79Axb7Rh");
                        json(200, &playlist.to_string())
                    }
                    _ => json(404, fixture!("error-404.json")),
                }
            })
            .await;
            let oauth = oauth(&server);
            let grant = oauth
                .exchange_code(
                    &concord_sdk_spotify_web_api::AuthorizationCode::new("code"),
                    common::REDIRECT,
                )
                .await
                .unwrap();
            let budget = Duration::from_millis(200);
            let options = SessionOptions::new().rate_limit_retries(5);
            let options = if session_deadline {
                options.deadline(budget)
            } else {
                options
            };
            let session = oauth.session(grant, options);
            let http = concord::reqwest::Client::builder()
                .no_proxy()
                .retry(concord::reqwest::retry::never())
                .redirect(concord::reqwest::redirect::Policy::none())
                .timeout(if session_deadline {
                    Duration::from_secs(5)
                } else {
                    budget
                })
                .build()
                .unwrap();
            let api = WebApi::new(
                http,
                WebApiConfig {
                    session: session.clone(),
                },
            )
            .with_base_url(server.url())
            .unwrap();
            let error = api
                .playlist(id())
                .add_items(batch())
                .send()
                .await
                .unwrap_err();
            let Error::UncertainWrite(cause) = &error else {
                panic!("expected uncertain addition: {error:?}")
            };
            if session_deadline {
                assert!(matches!(**cause, Error::Deadline));
            } else {
                assert!(error.concord().unwrap().transport().unwrap().is_timeout());
            }
            assert!(error.to_string().contains("re-read the playlist"));
            let playlist = api.playlist(id()).get().send().await.unwrap();
            assert_eq!(playlist.snapshot_id, "synthetic-new-snapshot+/=");
            let Some(concord_sdk_spotify_web_api::Playable::Track(track)) =
                &playlist.tracks.items[0].track
            else {
                panic!("expected applied track")
            };
            assert_eq!(
                track.id.as_ref().unwrap().as_str(),
                "4iV5W9uYEdYUVa79Axb7Rh"
            );
            let requests = server.requests();
            assert_eq!(
                at(&requests, TARGET).len(),
                1,
                "an uncertain addition must not be resent"
            );
            assert_eq!(
                at(&requests, "/api/token").len(),
                1,
                "a timeout must not trigger refresh"
            );
            assert!(requests.iter().all(|r| r.host == server.addr().to_string()));
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn a_deadline_during_token_acquisition_does_not_claim_an_uncertain_add() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.path(), "/api/token");
        json(200, fixture!("token-refresh-kept.json")).delay(Duration::from_secs(3))
    })
    .await;
    let session = oauth(&server).resume(
        RefreshToken::new("stored-refresh"),
        SessionOptions::new().deadline(Duration::from_millis(200)),
    );
    let error = api(&server, &session)
        .playlist(id())
        .add_items(batch())
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Deadline), "{error:?}");
    assert!(at(&server.requests(), TARGET).is_empty());
    assert_eq!(session.refresh_token().unwrap().expose(), "stored-refresh");
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_lost_or_unreadable_write_result_is_uncertain_without_replay() {
    for reply in [
        Reply::hang_up(),
        json(201, "{"),
        json(201, "{}"),
        Reply::text(201, "text/html", "<html>proxy</html>"),
    ] {
        let script = concord_testserver::Script::new([reply]);
        let server = TestServer::start(move |request: &Recorded| -> Reply {
            if request.path() == "/api/token" {
                json(200, fixture!("token-refresh-kept.json"))
            } else {
                script.next()
            }
        })
        .await;
        let session =
            oauth(&server).resume(RefreshToken::new("stored-refresh"), SessionOptions::new());
        let error = api(&server, &session)
            .playlist(id())
            .add_items(batch())
            .send()
            .await
            .unwrap_err();
        assert!(matches!(error, Error::UncertainWrite(_)), "{error:?}");
        assert!(
            error.concord().is_some(),
            "the original cause remains accessible"
        );
        assert_eq!(at(&server.requests(), TARGET).len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}
