//! Shared test support: fixtures, a manual clock and a session wired to a local server.
#![allow(dead_code, missing_docs)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord_sdk_spotify_web_api::{
    AppCredentials, ClientSecret, Grant, OAuth, RefreshToken, Session, SessionOptions, WebApi,
    WebApiConfig,
};
use concord_testserver::{Recorded, Reply, TestServer};

/// A fixture file's text.
#[macro_export]
macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/", $name))
    };
}

pub const CLIENT_ID: &str = "synthetic-client-id";
pub const CLIENT_SECRET: &str = "synthetic-client-secret";
/// `base64("synthetic-client-id:synthetic-client-secret")`, computed independently of the SDK.
pub const BASIC: &str = "Basic c3ludGhldGljLWNsaWVudC1pZDpzeW50aGV0aWMtY2xpZW50LXNlY3JldA==";
pub const REDIRECT: &str = "http://127.0.0.1:8888/callback";

/// A monotonic clock the test advances by hand.
#[derive(Clone)]
pub struct ManualClock(Arc<Mutex<Instant>>);

impl ManualClock {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    pub fn now(&self) -> Instant {
        *self.0.lock().unwrap()
    }

    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

pub fn app() -> AppCredentials {
    AppCredentials::new(CLIENT_ID, &ClientSecret::new(CLIENT_SECRET)).unwrap()
}

pub fn oauth(server: &TestServer) -> OAuth {
    OAuth::new(http(), app())
        .with_base_url(server.url())
        .unwrap()
}

pub fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .retry(concord::reqwest::retry::never())
        .redirect(concord::reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

pub fn api(server: &TestServer, session: &Session) -> WebApi {
    WebApi::new(
        http(),
        WebApiConfig {
            session: session.clone(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

pub fn options(clock: &ManualClock) -> SessionOptions {
    let clock = clock.clone();
    SessionOptions::new().clock(move || clock.now())
}

/// A session resumed from a stored refresh token (the first request refreshes).
pub fn resumed(server: &TestServer, refresh: &str, clock: &ManualClock) -> Session {
    oauth(server).resume(RefreshToken::new(refresh), options(clock))
}

/// A session started from a grant obtained by exchanging a code at `server`.
pub async fn granted(server: &TestServer, clock: &ManualClock) -> (OAuth, Grant, Session) {
    let oauth = oauth(server);
    let grant = oauth
        .exchange_code(
            &concord_sdk_spotify_web_api::AuthorizationCode::new("synthetic-code"),
            REDIRECT,
        )
        .await
        .unwrap();
    let session = oauth.session(grant.clone(), options(clock));
    (oauth, grant, session)
}

pub fn json(status: u16, body: &str) -> Reply {
    Reply::json(status, body)
}

/// Requests whose path is `path`.
pub fn at<'a>(requests: &'a [Recorded], path: &str) -> Vec<&'a Recorded> {
    requests.iter().filter(|r| r.path() == path).collect()
}

/// The form value of `name` in a token request.
pub fn form(request: &Recorded, name: &str) -> Option<String> {
    request
        .form_pairs()
        .into_iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
}
