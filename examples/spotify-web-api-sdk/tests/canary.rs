//! Secrets: ClientSecret, AuthorizationCode, State, input RefreshToken, decoded grant access
//! token and rotated RefreshToken (src/auth.rs:49-64,152-162,199-230), each distinct.
//! Basic is decoded by the wire helper. Public client_id is an unmarked public identifier
//! (AppCredentials::client_id documents it as public), so remains printable per triage.
//! State reaches the generated authorization URL; a loopback browser GET delivers it.
//! Response-only access/refresh tokens reach the real token decoder; refreshes send both
//! input and rotated refresh tokens, Web API reads/writes send the access token.
//! All seven scenarios cover read playlists, write add_items and token exchange. Error/header
//! echoes enter wrapper renderers that withhold them. Excludes native reqwest, expose(),
//! authorize_url(), explicit unredacted accessors and clear ErrorObject/OAuthErrorObject/model
//! values (including provider reason/error codes, which remain ordinary). No wire exemptions.

use concord_check_canary::Canaries;
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    concord_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
    c.assert_debug_absent("Prepared", &prepared);
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            let decoded = received.decode().await;
            c.assert_debug_absent("native decoded Result", &decoded);
            if let Err(e) = decoded {
                concord_error(c, &e);
            }
        }
        Err(e) => concord_error(c, &e),
    }
}

use concord_sdk_spotify_web_api::{
    AddItems, AppCredentials, AuthorizationCode, AuthorizeRequest, ClientSecret, Error, OAuth,
    RefreshToken, SessionOptions, SpotifyId, State, WebApi, WebApiConfig, pages,
};

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    if let Some(e) = e.concord() {
        concord_error(c, e);
    }
    if let Some(a) = e.api() {
        c.assert_debug_absent("API failure", a);
        c.assert_debug_absent("API metadata", a.meta());
        c.assert_debug_absent("API classification", &a.class());
        if let Err(e) = a.document() {
            concord_error(c, e);
        }
    }
    if let Some(a) = e.auth() {
        c.assert_debug_absent("auth error", a);
        c.assert_error_absent("auth error/source chain", a);
        if let Some(f) = a.failure() {
            c.assert_debug_absent("token failure", f);
            c.assert_debug_absent("token metadata", f.meta());
            if let Err(e) = f.document() {
                concord_error(c, e);
            }
        }
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("spotify-web-api");
    let client_secret = ClientSecret::new(c.issue());
    let code = AuthorizationCode::new(c.issue());
    let state = State::new(c.issue());
    let stored_refresh = RefreshToken::new(c.issue());
    let access = c.issue();
    let rotated = c.issue();
    let app = AppCredentials::new("public-app-id", &client_secret).unwrap();
    c.assert_debug_absent("client secret", &client_secret);
    c.assert_debug_absent("authorization code", &code);
    c.assert_debug_absent("CSRF state", &state);
    c.assert_debug_absent("stored refresh token", &stored_refresh);
    c.assert_debug_absent("app credentials", &app);
    let options = SessionOptions::new();
    c.assert_debug_absent("session options", &options);
    for case in 0..7 {
        let (echo, rotated_token) = (access.clone(), rotated.clone());
        let server = TestServer::start(move |r: &Recorded| {
            if r.path() == "/api/token" {
                return Reply::json(200, format!(r#"{{"access_token":"{echo}","token_type":"Bearer","expires_in":3600,"refresh_token":"{rotated_token}"}}"#)).header("x-private", &echo);
            }
            if r.path() == "/authorize" { return Reply::new(200); }
            let success = if r.method == "POST" { r#"{"snapshot_id":"ordinary-snapshot"}"# }
                else { r#"{"href":"ordinary","items":[],"limit":20,"offset":0,"total":0,"next":null,"previous":null}"# };
            let problem = format!(r#"{{"error":{{"status":400,"message":"{echo}","reason":"ordinary-reason"}}}}"#);
            reply(case, "application/json", success, &problem, &echo)
        }).await;
        let oauth = OAuth::new(http(), app.clone())
            .with_base_url(server.url())
            .unwrap();
        c.assert_debug_absent("OAuth client", &oauth);
        let grant = oauth
            .exchange_code(&code, "http://127.0.0.1/callback")
            .await
            .unwrap();
        c.assert_debug_absent("decoded grant", &grant);
        c.assert_debug_absent("decoded refresh token", &grant.refresh_token());
        assert_eq!(grant.refresh_token().unwrap().expose(), rotated);
        let session = oauth.session(grant, options.clone());
        c.assert_debug_absent("session", &session);
        let config = WebApiConfig {
            session: session.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = WebApi::new(http(), config)
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        let me = api.me();
        let playlist = api.playlist(SpotifyId::new("0SyntheticPlaylist0003").unwrap());
        let read = me.playlists();
        let items = AddItems::new(["spotify:track:0123456789012345678901"]).unwrap();
        c.assert_debug_absent("write input", &items);
        let write = playlist.add_items(items);
        c.assert_debug_absent("Web API client", &api);
        c.assert_debug_absent("me scope", &me);
        c.assert_debug_absent("playlist scope", &playlist);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        receive(&mut c, read.clone().prepare().unwrap()).await;
        receive(&mut c, write.clone().prepare().unwrap()).await;
        let r = read.clone().send().await;
        let w = write.send().await;
        c.assert_debug_absent("read Result", &r);
        c.assert_debug_absent("write Result", &w);
        assert_eq!(r.is_ok(), case == 0);
        assert_eq!(w.is_ok(), case == 0);
        if let Err(e) = &r {
            error(&mut c, e);
            if case == 1 {
                assert!(e.to_string().contains("400"));
                assert_eq!(e.api().unwrap().status(), 400);
                assert_eq!(e.api().unwrap().document().unwrap().message, access);
            }
        }
        if let Err(e) = &w {
            error(&mut c, e);
        }
        let mut walk = pages(read);
        c.assert_debug_absent("playlist traversal", &walk);
        let page = walk.next_page().await;
        c.assert_debug_absent("playlist page Result", &page);
        if let Err(e) = page {
            c.assert_error_absent("PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &walk.checkpoint());
        c.assert_debug_absent("traversal progress", &walk.progress());
        c.assert_debug_absent("traversal report", &walk.finish());
        if case == 0 {
            let request = AuthorizeRequest {
                redirect_uri: "http://127.0.0.1/callback".into(),
                scopes: vec![],
                state: state.clone(),
                show_dialog: false,
            };
            c.assert_debug_absent("authorization request", &request);
            let url = oauth.authorize_url(&request).unwrap();
            assert_eq!(
                url.query_pairs().find(|(k, _)| k == "state").unwrap().1,
                state.expose()
            );
            // Simulate the browser opening the public URL, solely against this loopback server.
            http().get(url).send().await.unwrap();
            let resumed = oauth.resume(stored_refresh.clone(), options.clone());
            c.assert_debug_absent("resumed session", &resumed);
            let resumed_api = WebApi::new(
                http(),
                WebApiConfig {
                    session: resumed.clone(),
                },
            )
            .with_base_url(server.url())
            .unwrap();
            c.assert_debug_absent(
                "resumed playlists",
                &resumed_api.me().playlists().send().await.unwrap(),
            );
            c.assert_debug_absent("session refresh token", &resumed.refresh_token());
            let grant = oauth.refresh(&RefreshToken::new(&rotated)).await.unwrap();
            c.assert_debug_absent("refreshed grant", &grant);
            c.assert_on_wire(&server.requests());
        }
        if case == 6 {
            assert!(server.requests().iter().all(|r| r.path() == "/api/token"));
        }
        server.shutdown().await;
    }
    // Token-endpoint failures must exercise its decoder and AuthError/TokenFailure surfaces.
    for case in 0..7 {
        let (echo, rotated_token) = (access.clone(), rotated.clone());
        let server = TestServer::start(move |_: &Recorded| {
            let success = format!(r#"{{"access_token":"{echo}","token_type":"Bearer","expires_in":3600,"refresh_token":"{rotated_token}"}}"#);
            let problem = format!(r#"{{"error":"invalid_grant","error_description":"{echo}"}}"#);
            reply(case, "application/json", &success, &problem, &echo)
        }).await;
        let oauth = OAuth::new(http(), app.clone())
            .with_base_url(if case == 6 { refused() } else { server.url() })
            .unwrap();
        c.assert_debug_absent("token OAuth client", &oauth);
        let result = oauth
            .exchange_code(&code, "http://127.0.0.1/callback")
            .await;
        c.assert_debug_absent("token Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Err(e) = result {
            error(&mut c, &e);
            if case == 1 {
                let a = e.auth().unwrap();
                assert!(a.requires_reauthorization());
                assert!(a.to_string().contains("400"));
                assert_eq!(
                    a.failure()
                        .unwrap()
                        .document()
                        .unwrap()
                        .error_description
                        .as_deref(),
                    Some(access.as_str())
                );
            }
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 400);
    println!(
        "spotify-web-api: {} renderings; Basic, token-decoder and browser positive controls passed",
        c.checked()
    );
}
