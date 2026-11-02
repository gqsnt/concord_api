//! The built-in `form` format (feature `form`, N08): `application/x-www-form-urlencoded` bodies
//! from any `Serialize` value through `serde_urlencoded`, and form responses. Runs in the
//! `--no-default-features --features form` lane (form in, text out) and, for form in / JSON out,
//! wherever `json` is enabled too.
#![deny(warnings)]

mod support;

use concord::{ErrorKind, reqwest};
use serde::{Deserialize, Serialize};
use support::{Reply, Server};

/// OAuth-style credentials.
#[derive(Debug, Clone, Serialize)]
pub struct Login {
    /// Grant type.
    pub grant_type: &'static str,
    /// User name.
    pub username: String,
    /// Optional scope: omitted when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}

/// A form-encoded token response.
#[derive(Debug, PartialEq, Deserialize, Serialize)]
pub struct Token {
    /// The token.
    pub access_token: String,
    /// Lifetime in seconds.
    pub expires_in: u32,
}

/// A value `serde_urlencoded` cannot encode (a nested structure).
#[derive(Debug, Clone, Serialize)]
pub struct Nested {
    /// Not a flat value.
    pub inner: Vec<u8>,
}

concord::api! {
    /// Form-encoded fixture: in place, as a default, and as a response format.
    pub client Auth {
        base "http://auth.invalid";

        POST login(form credentials: Login) -> text String { path "login"; }
        POST pairs(form fields: Vec<(String, String)>) { path "pairs"; }
        POST nested(form n: Nested) { path "nested"; }
        GET token() -> form Token { path "token"; }
        scope forms {
            request form;
            POST submit(body credentials?: Login) { path "submit"; }
        }
    }
}

async fn auth(reply: Reply) -> (Server, Auth) {
    let server = Server::reply(reply).await;
    let api = Auth::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    (server, api)
}

fn login() -> Login {
    Login {
        grant_type: "password",
        username: "ada lovelace&co".into(),
        scope: None,
    }
}

/// N08: a form body is URL-encoded (space as `+`, reserved characters escaped, `None` omitted)
/// with `Content-Type: application/x-www-form-urlencoded`; the response here is raw text.
#[tokio::test]
async fn form_in_text_out() {
    let (server, api) = auth(Reply::text(200, "welcome")).await;
    assert_eq!(api.login(login()).send().await.unwrap(), "welcome");
    let r = server.single();
    assert_eq!(r.body, b"grant_type=password&username=ada+lovelace%26co");
    assert_eq!(
        r.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
}

/// Pairs keep their order and repeated names; a `request form;` default reached through `body`
/// behaves the same, and an absent optional form body sends no body and no `Content-Type`.
#[tokio::test]
async fn pairs_and_form_default() {
    let (server, api) = auth(Reply::new(204)).await;
    let pairs = vec![
        ("b".to_owned(), "2".to_owned()),
        ("a".to_owned(), "1".to_owned()),
        ("b".to_owned(), "3".to_owned()),
    ];
    api.pairs(pairs).send().await.unwrap();
    api.forms()
        .submit()
        .set_credentials(login())
        .send()
        .await
        .unwrap();
    api.forms().submit().send().await.unwrap();
    let r = server.requests();
    assert_eq!(r[0].body, b"b=2&a=1&b=3");
    assert_eq!(r[1].body, b"grant_type=password&username=ada+lovelace%26co");
    assert_eq!(
        r[1].header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert!(r[2].body.is_empty());
    assert_eq!(r[2].header("content-type"), None);
}

/// A form response decodes into any `DeserializeOwned`; a malformed one is a content-free decode
/// error.
#[tokio::test]
async fn form_response() {
    let (_server, api) = auth(
        Reply::new(200)
            .header("content-type", "application/x-www-form-urlencoded")
            .body("access_token=t%2Bk&expires_in=60"),
    )
    .await;
    let token = api.token().send().await.unwrap();
    assert_eq!(
        token,
        Token {
            access_token: "t+k".into(),
            expires_in: 60
        }
    );
    let (_server, api) = auth(Reply::new(200).body("access_token=SECRET&expires_in=soon")).await;
    let error = api.token().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert_eq!(
        error.to_string(),
        "GET Auth.token: response not decoded: form decode error"
    );
    assert!(!format!("{error:?}").contains("SECRET"));
}

/// A value the form encoding cannot express is a build error before anything is sent.
#[test]
fn unencodable_form_is_a_build_error() {
    let error = Auth::new(reqwest::Client::new())
        .nested(Nested { inner: vec![1] })
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.to_string().contains("form encode error"), "{error}");
}

/// Form in, JSON out (an OAuth token endpoint): request and response formats are independent.
#[cfg(feature = "json")]
mod with_json {
    use super::*;

    concord::api! {
        /// A token endpoint.
        pub client OAuth {
            base "http://oauth.invalid";
            request form;
            response json;
            POST token(body credentials: Login) -> Token { path "token"; }
        }
    }

    #[tokio::test]
    async fn form_in_json_out() {
        let server = Server::reply(Reply::json(
            200,
            r#"{"access_token":"abc","expires_in":3600}"#,
        ))
        .await;
        let api = OAuth::new(reqwest::Client::new())
            .with_base_url(server.url())
            .unwrap();
        let token = api.token(login()).send().await.unwrap();
        assert_eq!(token.access_token, "abc");
        let r = server.single();
        assert_eq!(
            r.header("content-type"),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(r.body, b"grant_type=password&username=ada+lovelace%26co");
    }
}
