//! Sensitive values are sent but withheld from Concord-produced output.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

mod support;

use std::error::Error as _;

use concord::{ErrorKind, reqwest};
use support::{Reply, Server};

const SECRET: &str = "S3CR3T-v@lue/x";
/// `SECRET` as it appears percent-encoded in a path and form-encoded in a query.
const SECRET_PATH: &str = "S3CR3T-v%40lue%2Fx";
const SECRET_QUERY: &str = "S3CR3T-v%40lue%2Fx";

concord::api! {
    /// Sensitivity fixture.
    pub client Vault(
        #[sensitive] header("x-api-key") key: String,
        #[sensitive] query("sig") sig?: String,
        query("public") public?: String,
    ) {
        base "http://vault.invalid";
        headers { "authorization" = "Bearer {key}"; }

        scope tenant(#[sensitive] tenant: String) {
            subdomain "{tenant}";
            path "t/{tenant}";
            GET doc(#[sensitive] id: String, page_token?: String) {
                path "docs/{id}";
                query { "page_token" = page_token; }
            }
        }

        // The key is sensitive at the root; a public replacement does not un-mark it (sticky).
        GET replaced() { query { override "sig" = "public-sig"; } }
        // Name-based safety net: `api_key` is withheld without `#[sensitive]`.
        GET unmarked(api_key: String) { query { "api_key" = api_key; } }
    }
}

fn vault(base: &str) -> Vault {
    Vault::new(reqwest::Client::new(), VaultConfig { key: SECRET.into() })
        .set_sig(SECRET)
        .with_base_url(base)
        .unwrap()
}

fn assert_no_secret(text: &str) {
    for needle in [SECRET, SECRET_PATH, SECRET_QUERY, "S3CR3T"] {
        assert!(
            !text.contains(needle),
            "secret {needle:?} leaked in: {text}"
        );
    }
}

/// S01/S02: sensitive header and query values reach the wire unchanged.
#[tokio::test]
async fn s01_s02_sensitive_values_are_sent() {
    let server = Server::reply(Reply::new(204)).await;
    let api = vault(&server.url());
    api.replaced().send().await.unwrap();
    let request = server.single();
    assert_eq!(request.header("x-api-key"), Some(SECRET));
    assert_eq!(
        request.header("authorization").unwrap(),
        format!("Bearer {SECRET}")
    );
    assert_eq!(request.query(), Some("sig=public-sig"));
}

/// S02: a sensitive query value is on the wire but not in `Prepared`'s Debug.
#[tokio::test]
async fn s02_query_on_wire_redacted_in_debug() {
    let server = Server::reply(Reply::new(204)).await;
    let api = vault(&server.url());
    let prepared = api.clone().unmarked("plain-value").prepare().unwrap();
    let debug = format!("{prepared:?}");
    assert_no_secret(&debug);
    assert!(debug.contains("sig=<redacted>"), "{debug}");
    assert!(debug.contains("api_key=<redacted>"), "safety net: {debug}");
    assert!(debug.contains("x-api-key: <redacted>"), "{debug}");
    assert!(debug.contains("authorization: <redacted>"), "{debug}");
    prepared.send().await.unwrap();
    let request = server.single();
    assert!(
        request
            .query()
            .unwrap()
            .contains(&format!("sig={SECRET_QUERY}"))
    );
}

/// S03: sensitive path segments and host labels are withheld from `Prepared`'s Debug.
#[test]
fn s03_path_and_host_redacted() {
    let api = vault("http://vault.example.test");
    let prepared = api
        .tenant(SECRET.replace(['@', '/'], ""))
        .doc(SECRET)
        .set_page_token("p1")
        .prepare()
        .unwrap();
    let debug = format!("{prepared:?}");
    assert_no_secret(&debug);
    assert!(
        debug.contains("http://<redacted>.vault.example.test/t/<redacted>/docs/<redacted>?"),
        "{debug}"
    );
    // `page_token` is not on the exact safety list: it stays visible.
    assert!(debug.contains("page_token=p1"), "{debug}");
}

/// S04: a key marked sensitive anywhere on the chain stays redacted after a public override.
#[test]
fn s04_sensitivity_is_sticky() {
    let prepared = vault("http://v.test").replaced().prepare().unwrap();
    let debug = format!("{prepared:?}");
    assert!(debug.contains("sig=<redacted>"), "{debug}");
    assert!(!debug.contains("public-sig"), "{debug}");
    assert_eq!(prepared.request().url().query(), Some("sig=public-sig"));
}

/// Generated handle and endpoint `Debug` impls withhold sensitive values.
#[test]
fn generated_debug_redacts() {
    let api = vault("http://v.test").set_public("visible");
    let client = format!("{api:?}");
    assert_no_secret(&client);
    assert!(client.contains("key: <redacted>"), "{client}");
    assert!(client.contains("sig: Some(<redacted>)"), "{client}");
    assert!(client.contains(r#"public: Some("visible")"#), "{client}");
    let scope = api.tenant(SECRET);
    assert_no_secret(&format!("{scope:?}"));
    let endpoint = scope.doc(SECRET);
    let shown = format!("{endpoint:?}");
    assert_no_secret(&shown);
    assert!(shown.contains("page_token: None"), "{shown}");
}

/// S05: a build error caused by a sensitive value does not show it.
#[test]
fn s05_build_error_does_not_leak() {
    let api = Vault::new(
        reqwest::Client::new(),
        VaultConfig {
            key: format!("{SECRET}\n"),
        },
    );
    let error = api.replaced().prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_no_secret(&format!("{error} {error:?}"));

    let error = vault("http://v.test")
        .tenant(SECRET)
        .doc("x")
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_no_secret(&format!("{error} {error:?}"));
}

/// S05: transport errors carry no URL, so query/path secrets do not appear in `Display`, `Debug`
/// or the `source()` chain. (The raw reqwest error *would* show them — kept as a baseline.)
#[tokio::test]
async fn s05_transport_error_does_not_leak() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let api = vault(&format!("http://{addr}"));

    let error = api.clone().unmarked("x").send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    let mut shown = format!("{error} {error:?}");
    let mut source = error.source();
    while let Some(s) = source {
        shown.push_str(&format!(" | {s} {s:?}"));
        source = s.source();
    }
    assert_no_secret(&shown);

    // Baseline: reqwest's own error, from the same native request, prints the URL.
    let (client, request, _) = api.unmarked("x").prepare().unwrap().into_parts();
    let native = client.execute(request).await.unwrap_err();
    assert!(
        native.to_string().contains(SECRET_QUERY),
        "baseline changed: {native}"
    );
}

/// S06 (documented limit): the native request's own Debug is not redacted by Concord. Header
/// values marked sensitive print as `Sensitive` thanks to `http`, but the URL query does not.
#[test]
fn s06_native_request_debug_is_outside_the_guarantee() {
    let prepared = vault("http://v.test").unmarked("x").prepare().unwrap();
    let native = format!("{:?}", prepared.request());
    assert!(
        native.contains(SECRET_QUERY),
        "documented: the native URL is printed as is"
    );
    assert!(native.contains("\"x-api-key\": Sensitive"), "{native}");
    assert!(prepared.request().headers()["authorization"].is_sensitive());
}

concord::api! {
    /// C4-R-001 fixture: values that are not marked `#[sensitive]` but feed sensitive keys.
    pub client Effective(header("Authorization") token: String, #[sensitive] query("sig") sig?: String) {
        base "http://effective.invalid";
        GET mine(mine: String) { query { override "sig" = mine; } }
        GET plain(visible: String) { query { "visible" = visible; } }
    }
}

/// C4-R-001: generated `Debug` withholds values placed under a safety-list name or into a key made
/// sensitive elsewhere on the chain, even without `#[sensitive]`.
#[test]
fn c4_r_001_debug_uses_effective_sensitivity() {
    let api = Effective::new(
        reqwest::Client::new(),
        EffectiveConfig {
            token: SECRET.into(),
        },
    );
    let shown = format!("{api:?}");
    assert_no_secret(&shown);
    assert!(shown.contains("token: <redacted>"), "{shown}");
    let endpoint = api.clone().mine(SECRET);
    let shown = format!("{endpoint:?}");
    assert_no_secret(&shown);
    assert!(shown.contains("mine: <redacted>"), "{shown}");
    let shown = format!("{:?}", endpoint.prepare().unwrap());
    assert_no_secret(&shown);
    // A value with no sensitive placement stays visible.
    assert!(format!("{:?}", api.plain("hello")).contains(r#"visible: "hello""#));
}

concord::api! {
    /// ADV-1 fixture: a sensitive subdomain label.
    pub client Hidden(#[sensitive] tenant: String) {
        base "http://hidden.example.test";
        subdomain "{tenant}";
        GET ping();
    }
}

/// ADV-1: lower transport layers may print the host (e.g. a TLS name mismatch), so an endpoint
/// whose host contains a sensitive label exposes no `source()` chain; `transport()` still does.
#[tokio::test]
async fn adv_1_sensitive_host_hides_transport_source() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let http = reqwest::Client::builder()
        .resolve(
            "t0psecret.hidden.example.test",
            ([127, 0, 0, 1], port).into(),
        )
        .build()
        .unwrap();
    let api = Hidden::new(
        http,
        HiddenConfig {
            tenant: "t0psecret".into(),
        },
    )
    .with_base_url(format!("http://hidden.example.test:{port}"))
    .unwrap();
    let error = api.ping().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.source().is_none());
    assert!(error.transport().is_some());
    assert!(!format!("{error} {error:?}").contains("t0psecret"));
}

concord::api! {
    /// ADV-2 fixture: values withheld by the safety list, then used in other placements.
    pub client Spread(header("Authorization") token: String) {
        base "http://spread.invalid/v1";
        scope keys { path "keys/{client.token}"; GET get(); }
        GET q(api_key: String) { query { "api_key" = api_key; "echo" = "{api_key}"; } }
    }
}

/// ADV-2: a value Concord withholds anywhere is withheld everywhere it is placed.
#[test]
fn adv_2_effective_sensitivity_reaches_every_placement() {
    let api = Spread::new(
        reqwest::Client::new(),
        SpreadConfig {
            token: SECRET.into(),
        },
    );
    let shown = format!("{:?}", api.keys().get().prepare().unwrap());
    assert_no_secret(&shown);
    assert!(shown.contains("/v1/keys/<redacted>"), "{shown}");
    let shown = format!("{:?}", api.q(SECRET).prepare().unwrap());
    assert_no_secret(&shown);
    assert!(
        shown.contains("api_key=<redacted>&echo=<redacted>"),
        "{shown}"
    );
}

concord::api! {
    /// Safety-list normalization: lowercase, `_` read as `-`, exact names only.
    pub client Names(header("X-API-KEY") key: String) {
        base "http://names.invalid";
        GET e(a: String, b: String) { query { "Client_Secret" = a; "secret_id" = b; } }
    }
}

/// SEMANTICS §9: names are compared after lowercasing and mapping `_` to `-`, and must match
/// exactly (`secret_id` is not `secret`).
#[test]
fn safety_list_normalization_is_exact() {
    let p = Names::new(
        reqwest::Client::new(),
        NamesConfig {
            key: "k-123".into(),
        },
    )
    .e("A-VALUE", "B-VALUE")
    .prepare()
    .unwrap();
    let shown = format!("{p:?}");
    assert!(shown.contains("Client_Secret=<redacted>"), "{shown}");
    assert!(shown.contains("secret_id=B-VALUE"), "{shown}");
    assert!(shown.contains("x-api-key: <redacted>"), "{shown}");
    assert!(
        !shown.contains("A-VALUE") && !shown.contains("k-123"),
        "{shown}"
    );
}

/// S05: build errors have no `source()`.
#[test]
fn build_errors_have_no_source() {
    let error = vault("http://v.test")
        .tenant(SECRET)
        .doc("x")
        .prepare()
        .unwrap_err();
    assert!(error.source().is_none());
}
