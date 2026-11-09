//! Wire-level request shapes, every documented error class, broken bodies and redaction.
//! Every server here is a local stand-in; nothing contacts Spotify.

mod common;

use std::time::Duration;

use common::{
    BASIC, CLIENT_SECRET, ManualClock, REDIRECT, api, app, at, form, granted, json, oauth,
};
use concord::ErrorKind;
use concord::reqwest::StatusCode;
use concord_sdk_spotify_web_api::concord_pagination::Resume;
use concord_sdk_spotify_web_api::{
    ApiClass, AppCredentials, AuthError, AuthorizationCode, AuthorizeRequest, ClientSecret, Error,
    Market, OAuth, RefreshToken, SessionOptions, SpotifyId, State, WebApi, WebApiConfig, pages,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};

/// A reply factory for one case.
type Responder = Box<dyn Fn() -> Reply + Send + Sync>;

const ARTIST: &str = "0SyntheticArtist000003";
const PLAYLIST: &str = "0SyntheticPlaylist0003";

fn id(text: &str) -> SpotifyId {
    SpotifyId::new(text).unwrap()
}

/// Answers the code exchange; every other request with `reply`.
fn with_grant(
    reply: impl Fn(&Recorded) -> Reply + Send + Sync + 'static,
) -> impl Fn(&Recorded) -> Reply + Send + Sync + 'static {
    move |request: &Recorded| {
        if request.path() == "/api/token"
            && form(request, "grant_type").as_deref() == Some("authorization_code")
        {
            return json(200, fixture!("token-grant.json"));
        }
        reply(request)
    }
}

// ---------------------------------------------------------------------------------------------
// Request shapes
// ---------------------------------------------------------------------------------------------

#[test]
fn the_authorization_url_is_built_without_sending_anything() {
    let oauth = OAuth::new(common::http(), app());
    let request = AuthorizeRequest {
        redirect_uri: REDIRECT.to_owned(),
        scopes: vec![
            "playlist-read-private".to_owned(),
            "playlist-read-collaborative".to_owned(),
        ],
        state: State::new("synthetic-state"),
        show_dialog: true,
    };
    let url = oauth.authorize_url(&request).unwrap();
    assert_eq!(
        url.as_str(),
        "https://accounts.spotify.com/authorize?client_id=synthetic-client-id\
         &redirect_uri=http%3A%2F%2F127.0.0.1%3A8888%2Fcallback\
         &scope=playlist-read-private+playlist-read-collaborative\
         &state=synthetic-state&show_dialog=true&response_type=code"
    );

    let minimal = AuthorizeRequest {
        scopes: Vec::new(),
        show_dialog: false,
        ..request.clone()
    };
    let url = oauth.authorize_url(&minimal).unwrap();
    assert_eq!(
        url.query(),
        Some(
            "client_id=synthetic-client-id&redirect_uri=http%3A%2F%2F127.0.0.1%3A8888%2Fcallback\
             &state=synthetic-state&response_type=code"
        )
    );

    for bad in ["/callback", "https://app.example/cb#frag"] {
        let request = AuthorizeRequest {
            redirect_uri: bad.to_owned(),
            ..request.clone()
        };
        assert!(matches!(
            oauth.authorize_url(&request),
            Err(Error::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn the_code_exchange_and_the_refresh_are_basic_authenticated_forms() {
    let server =
        TestServer::start(
            |request: &Recorded| match form(request, "grant_type").as_deref() {
                Some("authorization_code") => json(200, fixture!("token-grant.json")),
                _ => json(200, fixture!("token-refresh-kept.json")),
            },
        )
        .await;
    let oauth = oauth(&server);
    let grant = oauth
        .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
        .await
        .unwrap();
    assert_eq!(grant.expires_in(), Duration::from_secs(3600));
    assert_eq!(
        grant.refresh_token().unwrap().expose(),
        "synthetic-refresh-1"
    );
    assert_eq!(
        grant.scope(),
        Some("playlist-read-private playlist-read-collaborative")
    );
    let refreshed = oauth.refresh(grant.refresh_token().unwrap()).await.unwrap();
    assert!(
        refreshed.refresh_token().is_none(),
        "not rotated: keep the old one"
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for request in &requests {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/api/token");
        assert_eq!(request.header("authorization"), Some(BASIC));
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    assert_eq!(
        requests[0].form_pairs(),
        [
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), "synthetic-code".to_owned()),
            ("redirect_uri".to_owned(), REDIRECT.to_owned()),
        ]
    );
    assert_eq!(
        requests[1].form_pairs(),
        [
            ("grant_type".to_owned(), "refresh_token".to_owned()),
            ("refresh_token".to_owned(), "synthetic-refresh-1".to_owned()),
        ]
    );
}

#[tokio::test]
async fn web_api_requests_have_the_declared_shapes() {
    let server = TestServer::start(with_grant(|request: &Recorded| match request.path() {
        "/v1/search" => json(200, fixture!("search-artists-1.json")),
        "/v1/artists/0SyntheticArtist000003" => json(200, fixture!("artist.json")),
        "/v1/me/playlists" => json(200, fixture!("me-playlists-1.json")),
        "/v1/playlists/0SyntheticPlaylist0003" => json(200, fixture!("playlist.json")),
        _ => json(200, fixture!("playlist-items-2.json")),
    }))
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);

    api.search_artists("Nina Simone & co")
        .set_market(Market::country("se").unwrap())
        .set_limit(2)
        .set_offset(0)
        .send()
        .await
        .unwrap();
    api.artist(id(ARTIST)).send().await.unwrap();
    api.me().playlists().set_limit(50).send().await.unwrap();
    api.playlist(id(PLAYLIST)).get().send().await.unwrap();
    api.playlist(id(PLAYLIST))
        .items()
        .set_market(Market::from_token())
        .set_limit(4)
        .set_offset(4)
        .send()
        .await
        .unwrap();

    let requests = server.requests();
    let api_requests: Vec<_> = requests
        .iter()
        .filter(|r| r.path().starts_with("/v1/"))
        .collect();
    let targets: Vec<&str> = api_requests.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        [
            "/v1/search?q=Nina+Simone+%26+co&market=SE&limit=2&offset=0&type=artist",
            "/v1/artists/0SyntheticArtist000003",
            "/v1/me/playlists?limit=50",
            "/v1/playlists/0SyntheticPlaylist0003",
            "/v1/playlists/0SyntheticPlaylist0003/tracks?market=from_token&limit=4&offset=4",
        ]
    );
    for request in api_requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.header("authorization"),
            Some("Bearer synthetic-access-1")
        );
        assert_eq!(request.header("accept"), Some("application/json"));
        assert!(request.body.is_empty());
    }
}

#[test]
fn prepare_builds_the_declared_request_without_authorization() {
    let server_url = "http://127.0.0.1:9";
    let oauth = OAuth::new(common::http(), app());
    let session = oauth.resume(
        RefreshToken::new("synthetic-refresh-x"),
        SessionOptions::new(),
    );
    let api = WebApi::new(common::http(), WebApiConfig { session })
        .with_base_url(server_url)
        .unwrap();
    let prepared = api.artist(id(ARTIST)).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "http://127.0.0.1:9/v1/artists/0SyntheticArtist000003"
    );
    assert!(prepared.request().headers().get("authorization").is_none());
    WebApi::validate_header_names().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Documented error classes
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn every_documented_status_class_is_mapped_with_its_document() {
    // (status, content type, body, expected class, document message or None when unreadable)
    let cases: Vec<(u16, &str, &str, ApiClass, Option<&str>)> = vec![
        (
            400,
            "application/json",
            fixture!("error-400.json"),
            ApiClass::BadRequest,
            Some("invalid id"),
        ),
        (
            403,
            "application/json",
            fixture!("error-403.json"),
            ApiClass::Forbidden,
            Some("Forbidden"),
        ),
        (
            404,
            "application/json",
            fixture!("error-404.json"),
            ApiClass::NotFound,
            Some("Resource not found"),
        ),
        (
            429,
            "application/json",
            fixture!("error-429-rate.json"),
            ApiClass::RateLimited,
            Some("API rate limit exceeded"),
        ),
        (
            429,
            "application/json",
            fixture!("error-429-quota.json"),
            ApiClass::QuotaExceeded,
            Some("Too many requests"),
        ),
        (
            500,
            "application/json",
            fixture!("error-500.json"),
            ApiClass::Server,
            Some("Server error"),
        ),
        (
            502,
            "text/html",
            fixture!("proxy-502.html"),
            ApiClass::Server,
            None,
        ),
        (503, "application/json", "", ApiClass::Server, None),
        (
            418,
            "application/json",
            fixture!("error-400.json"),
            ApiClass::Other,
            Some("invalid id"),
        ),
    ];
    for (status, media, body, class, message) in cases {
        let server = TestServer::start(with_grant(move |_: &Recorded| {
            Reply::text(status, media, body)
        }))
        .await;
        let clock = ManualClock::new();
        let (_oauth, _grant, session) = granted(&server, &clock).await;
        let api = api(&server, &session);

        let error = api.artist(id(ARTIST)).send().await.unwrap_err();
        let failure = error.api().unwrap_or_else(|| panic!("{status}: {error:?}"));
        assert_eq!(failure.class(), class, "{status}");
        assert_eq!(failure.status(), status);
        assert_eq!(
            failure.meta().status(),
            StatusCode::from_u16(status).unwrap()
        );
        match message {
            Some(message) => assert_eq!(failure.document().unwrap().message, message),
            None => assert!(
                failure.document().is_err(),
                "{status}: the status stays authoritative"
            ),
        }
        assert_eq!(error.may_be_development_mode(), status == 403);
        // None of these refreshes or replays.
        let requests = server.requests();
        assert_eq!(at(&requests, "/api/token").len(), 1, "{status}");
        assert_eq!(
            at(&requests, "/v1/artists/0SyntheticArtist000003").len(),
            1,
            "{status}"
        );
    }
}

#[tokio::test]
async fn a_documented_400_example_is_read_verbatim() {
    // The documentation's example: a non-existent track id answers 400 `invalid id`.
    let server = TestServer::start(with_grant(|_: &Recorded| {
        json(400, fixture!("error-400.json"))
    }))
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let error = api(&server, &session)
        .artist(id(ARTIST))
        .send()
        .await
        .unwrap_err();
    let document = error.api().unwrap().document().unwrap();
    assert_eq!(
        (document.status, document.message.as_str()),
        (400, "invalid id")
    );
    assert_eq!(document.reason, None);
    assert_eq!(error.to_string(), "the Web API answered 400");
}

#[tokio::test]
async fn retry_after_is_read_strictly() {
    let cases: Vec<(Vec<&str>, Option<Duration>)> = vec![
        (vec!["2"], Some(Duration::from_secs(2))),
        (vec!["Sun, 06 Nov 1994 08:49:37 GMT"], Some(Duration::ZERO)),
        (vec!["soon"], None),
        (vec!["+3"], None),
        (vec!["1", "2"], None),
        (vec![], None),
    ];
    for (values, expected) in cases {
        let header_values: Vec<String> = values.iter().map(|v| (*v).to_owned()).collect();
        let server = TestServer::start(with_grant(move |_: &Recorded| {
            let mut reply = json(429, fixture!("error-429-rate.json"));
            for value in &header_values {
                reply = reply.header("retry-after", value.clone());
            }
            reply
        }))
        .await;
        let clock = ManualClock::new();
        let (_oauth, _grant, session) = granted(&server, &clock).await;
        let error = api(&server, &session)
            .artist(id(ARTIST))
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.api_class(), Some(ApiClass::RateLimited));
        assert_eq!(error.retry_after(), expected, "{values:?}");
        let parsed = error.api().unwrap().retry_after();
        match values.len() {
            0 => assert_eq!(parsed, Ok(None)),
            _ if expected.is_some() => assert_eq!(parsed, Ok(expected)),
            _ => assert!(parsed.is_err(), "{values:?}"),
        }
    }
}

#[tokio::test]
async fn a_retry_after_above_the_ceiling_is_not_waited_for() {
    let server = TestServer::start(with_grant(|_: &Recorded| {
        json(429, fixture!("error-429-rate.json")).header("retry-after", "30")
    }))
    .await;
    let oauth = oauth(&server);
    let grant = oauth
        .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
        .await
        .unwrap();
    let session = oauth.session(
        grant,
        SessionOptions::new()
            .rate_limit_retries(3)
            .max_retry_wait(Duration::from_secs(5)),
    );
    let started = std::time::Instant::now();
    let error = api(&server, &session)
        .artist(id(ARTIST))
        .send()
        .await
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(error.retry_after(), Some(Duration::from_secs(30)));
    assert_eq!(
        at(&server.requests(), "/v1/artists/0SyntheticArtist000003").len(),
        1
    );
}

#[tokio::test]
async fn the_session_deadline_bounds_a_call() {
    let server = TestServer::start(with_grant(|_: &Recorded| {
        json(200, fixture!("artist.json")).delay(Duration::from_secs(3))
    }))
    .await;
    let oauth = oauth(&server);
    let grant = oauth
        .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
        .await
        .unwrap();
    let session = oauth.session(
        grant,
        SessionOptions::new().deadline(Duration::from_millis(200)),
    );
    let started = std::time::Instant::now();
    let error = api(&server, &session)
        .artist(id(ARTIST))
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Deadline), "{error:?}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn token_endpoint_error_documents_are_classified() {
    let cases: Vec<(u16, &str, &str, Option<&str>, bool)> = vec![
        (
            400,
            "application/json",
            fixture!("oauth-invalid-grant.json"),
            Some("invalid_grant"),
            true,
        ),
        (
            401,
            "application/json",
            fixture!("oauth-invalid-client.json"),
            Some("invalid_client"),
            false,
        ),
        (503, "text/html", fixture!("wrong-media.html"), None, false),
    ];
    for (status, media, body, code, reauthorize) in cases {
        let server = TestServer::start(move |_: &Recorded| Reply::text(status, media, body)).await;
        let error = oauth(&server)
            .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
            .await
            .unwrap_err();
        let auth = error.auth().unwrap();
        let failure = auth.failure().unwrap();
        assert_eq!(failure.status(), status);
        assert_eq!(failure.code(), code);
        assert_eq!(auth.requires_reauthorization(), reauthorize);
        if code == Some("invalid_client") {
            // The documentation's example, verbatim.
            assert_eq!(
                failure.document().unwrap().error_description.as_deref(),
                Some("Invalid client secret")
            );
        }
        if code.is_none() {
            assert_eq!(failure.document().unwrap_err().kind(), ErrorKind::Metadata);
        }
    }
}

#[tokio::test]
async fn the_token_document_contract_is_checked() {
    let cases: Vec<(&str, &str)> = vec![
        (
            r#"{"access_token":"a","token_type":"mac","expires_in":3600}"#,
            "the token type is not Bearer",
        ),
        (
            r#"{"access_token":"a","token_type":"Bearer","expires_in":0}"#,
            "the access token has no lifetime",
        ),
        (
            r#"{"access_token":"a b","token_type":"Bearer","expires_in":3600}"#,
            "the access token is not a valid bearer token",
        ),
    ];
    for (body, reason) in cases {
        let server = TestServer::start(move |_: &Recorded| json(200, body)).await;
        let error = oauth(&server)
            .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
            .await
            .unwrap_err();
        assert!(
            matches!(error.auth(), Some(AuthError::Protocol(r)) if *r == reason),
            "{body}: {error:?}"
        );
    }

    // A refresh whose lifetime is within the refresh margin is refused (it could never be reused).
    let server = TestServer::start(|_: &Recorded| {
        json(
            200,
            r#"{"access_token":"short","token_type":"Bearer","expires_in":30}"#,
        )
    })
    .await;
    let session = oauth(&server).resume(
        RefreshToken::new("synthetic-refresh-x"),
        SessionOptions::new(),
    );
    let error = api(&server, &session)
        .artist(id(ARTIST))
        .send()
        .await
        .unwrap_err();
    assert!(
        matches!(error.auth(), Some(AuthError::Protocol(_))),
        "{error:?}"
    );
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-x"
    );
    assert!(at(&server.requests(), "/v1/artists/0SyntheticArtist000003").is_empty());

    // An empty replacement refresh token counts as absent: the stored one is kept.
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/api/token" => json(
            200,
            r#"{"access_token":"fresh","token_type":"bearer","expires_in":3600,"refresh_token":""}"#,
        ),
        _ => json(200, fixture!("artist.json")),
    })
    .await;
    let session = oauth(&server).resume(
        RefreshToken::new("synthetic-refresh-x"),
        SessionOptions::new(),
    );
    api(&server, &session)
        .artist(id(ARTIST))
        .send()
        .await
        .unwrap();
    assert_eq!(
        session.refresh_token().unwrap().expose(),
        "synthetic-refresh-x"
    );
    let fetched = at(&server.requests(), "/v1/artists/0SyntheticArtist000003")[0].clone();
    assert_eq!(fetched.header("authorization"), Some("Bearer fresh"));
}

// ---------------------------------------------------------------------------------------------
// Broken bodies
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn malformed_truncated_oversized_and_wrong_media_bodies_are_concord_errors() {
    let oversized = || {
        // 5 MiB of JSON whitespace in front of a value: longer than the 4 MiB bound.
        let mut sent = 0usize;
        Body::Generate(
            Box::new(move || {
                if sent >= 5 * 1024 * 1024 {
                    return None;
                }
                sent += 64 * 1024;
                Some(vec![b' '; 64 * 1024].into())
            }),
            None,
        )
    };
    let cases: Vec<(&str, Responder, ErrorKind)> = vec![
        (
            "malformed",
            Box::new(|| json(200, fixture!("malformed.json"))),
            ErrorKind::Decode,
        ),
        ("empty", Box::new(|| json(200, "")), ErrorKind::Decode),
        (
            "wrong shape",
            Box::new(|| json(200, r#"{"id":"0SyntheticArtist000003"}"#)),
            ErrorKind::Decode,
        ),
        (
            "invalid id in body",
            Box::new(|| {
                json(
                    200,
                    &fixture!("artist.json").replace("0SyntheticArtist000003", "bad:id"),
                )
            }),
            ErrorKind::Decode,
        ),
        (
            "wrong media",
            Box::new(|| Reply::text(200, "text/html", fixture!("wrong-media.html"))),
            ErrorKind::Metadata,
        ),
        (
            "unlabelled",
            Box::new(|| Reply::new(200).body(fixture!("artist.json"))),
            ErrorKind::Metadata,
        ),
        (
            "truncated",
            Box::new(|| {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![
                        fixture!("artist.json").as_bytes()[..40].to_vec().into(),
                    ]))
            }),
            ErrorKind::Transport,
        ),
        (
            "oversized",
            Box::new(move || {
                Reply::new(200)
                    .header("content-type", "application/json")
                    .with_body(oversized())
            }),
            ErrorKind::Limit,
        ),
    ];
    for (name, reply, kind) in cases {
        let server = TestServer::start(with_grant(move |_: &Recorded| reply())).await;
        let clock = ManualClock::new();
        let (_oauth, _grant, session) = granted(&server, &clock).await;
        let error = api(&server, &session)
            .artist(id(ARTIST))
            .send()
            .await
            .unwrap_err();
        let concord = error
            .concord()
            .unwrap_or_else(|| panic!("{name}: {error:?}"));
        assert_eq!(concord.kind(), kind, "{name}");
        assert!(matches!(error, Error::Concord(_)));
        if kind != ErrorKind::Transport {
            assert_eq!(
                concord.head().unwrap().status(),
                StatusCode::OK,
                "{name} keeps the head"
            );
        }
    }
}

#[tokio::test]
async fn a_wrong_media_token_response_is_an_authentication_failure() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::text(200, "text/plain", fixture!("token-grant.json"))
    })
    .await;
    let error = oauth(&server)
        .exchange_code(&AuthorizationCode::new("synthetic-code"), REDIRECT)
        .await
        .unwrap_err();
    assert!(matches!(error.auth(), Some(AuthError::Concord(e)) if e.kind() == ErrorKind::Metadata));
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Metadata);
}

#[tokio::test]
async fn pages_that_break_the_paging_contract_are_refused() {
    // A page that does not start at the requested offset.
    let shifted = fixture!("me-playlists-2.json").replace("\"offset\": 2", "\"offset\": 1");
    let server = TestServer::start(with_grant(move |request: &Recorded| {
        match request.query_all("offset").first().map(String::as_str) {
            Some("2") => json(200, &shifted),
            _ => json(200, fixture!("me-playlists-1.json")),
        }
    }))
    .await;
    let clock = ManualClock::new();
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);
    let incomplete = pages(api.me().playlists().set_limit(2))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Protocol(_))
    ));
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(2)));

    // A next link with a zero limit cannot make progress.
    let stuck = fixture!("me-playlists-1.json").replace("\"limit\": 2", "\"limit\": 0");
    let server = TestServer::start(with_grant(move |_: &Recorded| json(200, &stuck))).await;
    let (_oauth, _grant, session) = granted(&server, &clock).await;
    let stuck_api = common::api(&server, &session);
    let error = pages(stuck_api.me().playlists())
        .starting_at(Resume::Start)
        .next_page()
        .await
        .unwrap_err();
    assert!(matches!(error.provider_error(), Some(Error::Protocol(_))));
    assert_eq!(at(&server.requests(), "/v1/me/playlists").len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Inputs and redaction
// ---------------------------------------------------------------------------------------------

#[test]
fn inputs_are_validated_before_any_request() {
    for bad in [
        "",
        "spotify:artist:0SyntheticArtist000003",
        "a/b",
        "..",
        "ä",
        &"x".repeat(65),
    ] {
        assert!(
            matches!(SpotifyId::new(bad), Err(Error::InvalidInput(_))),
            "{bad:?}"
        );
    }
    assert_eq!(id(ARTIST).as_str(), ARTIST);
    assert_eq!(Market::country("gb").unwrap().to_string(), "GB");
    assert_eq!(Market::from_token().to_string(), "from_token");
    for bad in ["", "G", "GBR", "1A"] {
        assert!(Market::country(bad).is_err(), "{bad:?}");
    }
    let secret = ClientSecret::new(CLIENT_SECRET);
    assert!(AppCredentials::new("", &secret).is_err());
    assert!(AppCredentials::new("client:id", &secret).is_err());
    assert!(AppCredentials::new("client", &ClientSecret::new("line\nbreak")).is_err());
}

#[tokio::test]
async fn secrets_never_reach_debug_or_display() {
    const SECRETS: [&str; 6] = [
        CLIENT_SECRET,
        "c3ludGhldGljLWNsaWVudC1pZDpzeW50aGV0aWMtY2xpZW50LXNlY3JldA",
        "synthetic-access-1",
        "synthetic-refresh-1",
        "synthetic-code",
        "synthetic-state",
    ];
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/api/token" if form(request, "grant_type").as_deref() == Some("authorization_code") => {
            json(200, fixture!("token-grant.json"))
        }
        "/api/token" => json(400, fixture!("oauth-invalid-grant.json")),
        _ => json(401, fixture!("error-401.json")),
    })
    .await;
    let clock = ManualClock::new();
    let (oauth, grant, session) = granted(&server, &clock).await;
    let api = api(&server, &session);
    // 401, then the refresh is refused with invalid_grant.
    let auth_error = api.artist(id(ARTIST)).send().await.unwrap_err();
    assert!(matches!(
        auth_error.auth(),
        Some(AuthError::InvalidGrant(_))
    ));

    let server_401 = TestServer::start(|_: &Recorded| json(401, fixture!("error-401.json"))).await;
    let api_error = {
        let session = oauth.session(grant.clone(), SessionOptions::new());
        let api = WebApi::new(common::http(), WebApiConfig { session })
            .with_base_url(server_401.url())
            .unwrap();
        // The grant's token is used, refused, and the refresh (to the first server's accounts
        // stand-in) is refused too: either way no secret may be printed.
        api.artist(id(ARTIST)).send().await.unwrap_err()
    };

    let request = AuthorizeRequest {
        redirect_uri: REDIRECT.to_owned(),
        scopes: vec!["playlist-read-private".to_owned()],
        state: State::new("synthetic-state"),
        show_dialog: false,
    };
    let rendered = [
        format!("{:?}", app()),
        format!("{:?}", ClientSecret::new(CLIENT_SECRET)),
        format!("{:?}", RefreshToken::new("synthetic-refresh-1")),
        format!("{:?}", AuthorizationCode::new("synthetic-code")),
        format!("{:?}", State::new("synthetic-state")),
        format!("{request:?}"),
        format!("{grant:?}"),
        format!("{oauth:?}"),
        format!("{session:?}"),
        format!("{api:?}"),
        format!("{:?}", SessionOptions::new()),
        format!("{auth_error:?}"),
        auth_error.to_string(),
        format!("{api_error:?}"),
        api_error.to_string(),
        format!("{:?}", api.artist(id(ARTIST)).prepare().unwrap()),
        format!("{:?}", api.artist(id(ARTIST))),
    ];
    for text in &rendered {
        for secret in SECRETS {
            assert!(!text.contains(secret), "{secret} leaked in {text}");
        }
    }
    assert!(format!("{grant:?}").contains("expires_in"));
    assert!(auth_error.to_string().contains("reauthorization required"));
}
