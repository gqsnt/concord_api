//! D15 (refinement): sensitivity surfaces, each with a positive control. Every leak test shows
//! the dangerous data present somewhere (wire, native error chain, decoder source) and absent
//! from Concord-produced output (`Display`, `Debug`, `source()`).
//!
//! Ported from the core track's `d15_surfaces.rs` (written against 8f7adc2) to the current
//! syntax: `Client::new(http, Config { .. })`, `body b: T via R`, `Encode<T>`.

mod support;

use std::error::Error as _;
use std::sync::Arc;

use concord::{BodyError, ErrorKind, Json, reqwest};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use support::{Reply, Server};

const CA: &[u8] = include_bytes!("support/tls/ca.pem");
const CERT: &[u8] = include_bytes!("support/tls/cert.pem");
const KEY: &[u8] = include_bytes!("support/tls/key.pem");

fn chain(error: Option<&(dyn std::error::Error + 'static)>) -> String {
    let mut out = String::new();
    let mut source = error;
    while let Some(s) = source {
        out.push_str(&format!(" | {s} {s:?}"));
        source = s.source();
    }
    out
}

fn native(error: &concord::Error) -> String {
    chain(
        error
            .transport()
            .map(|e| e as &(dyn std::error::Error + 'static)),
    )
}

fn shown(error: &concord::Error) -> String {
    format!("{error} {error:?}{}", chain(error.source()))
}

concord::api! {
    /// ADV-1 fixture over plain HTTP.
    pub client Hidden(#[sensitive] tenant: String) {
        base "http://hidden.example.test";
        subdomain "{tenant}";
        GET ping();
    }
}

/// ADV-1 over plain HTTP (connection refused) has NO positive control: the native chain does not
/// contain the sensitive label either, so a "does not leak" assertion there would hold even when
/// nothing could leak. Kept to document why the TLS variant below exists.
#[tokio::test]
async fn adv_1_plain_http_scenario_has_no_positive_control() {
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
    let native = native(&error);
    assert!(!native.contains("t0psecret"), "native chain: {native}");
    assert!(!shown(&error).contains("t0psecret"));
}

concord::api! {
    /// ADV-1 with TLS: the sensitive label becomes part of the server name.
    pub client HiddenTls(#[sensitive] tenant: String) {
        base "https://localhost";
        subdomain "{tenant}";
        GET ping();
    }
}

/// ADV-1 with a positive control: a TLS name mismatch for `t0psecret.localhost` (the certificate
/// is for `localhost`) puts the sensitive label into the native error chain; Concord's `Display`,
/// `Debug` and (absent) `source()` do not show it.
#[tokio::test]
async fn adv_1_tls_name_mismatch_positive_control() {
    let config = tokio_rustls::rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from_pem_slice(CERT).unwrap()],
            PrivateKeyDer::from_pem_slice(KEY).unwrap(),
        )
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let _ = acceptor.accept(tcp).await;
        }
    });
    let http = reqwest::Client::builder()
        .tls_certs_only([reqwest::Certificate::from_pem(CA).unwrap()])
        .resolve("t0psecret.localhost", ([127, 0, 0, 1], port).into())
        .build()
        .unwrap();
    let api = HiddenTls::new(
        http,
        HiddenTlsConfig {
            tenant: "t0psecret".into(),
        },
    )
    .with_base_url(format!("https://localhost:{port}"))
    .unwrap();
    let error = api.ping().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.source().is_none());
    let native = native(&error);
    assert!(native.contains("t0psecret"), "positive control: {native}");
    let shown = shown(&error);
    assert!(!shown.contains("t0psecret"), "{shown}");
}

concord::api! {
    /// S07 fixture.
    pub client Based {
        base "http://based.invalid";
        GET ping();
    }
}

/// S07: the base URL is never treated as sensitive. Pinned: a caller-supplied base with private
/// data appears in client `Debug` and `Prepared` `Debug` (positive control); `with_base_url`
/// errors do not echo it; transport errors carry no URL although the native error does.
#[tokio::test]
async fn s07_runtime_base_is_shown_but_not_echoed_by_errors() {
    let api = Based::new(reqwest::Client::new())
        .with_base_url("http://acct-PRIVATE.example.test/u/PRIVATE")
        .unwrap();
    // Host canonicalized to lowercase by URL parsing.
    let client_debug = format!("{api:?}");
    assert!(
        client_debug.contains("acct-private.example.test/u/PRIVATE"),
        "{client_debug}"
    );
    let prepared = format!("{:?}", api.clone().ping().prepare().unwrap());
    assert!(prepared.contains("/u/PRIVATE"), "{prepared}");

    let bad = Based::new(reqwest::Client::new())
        .with_base_url("http://acct-PRIVATE.example.test/u/PRIVATE?x=1")
        .unwrap_err();
    assert!(!format!("{bad} {bad:?}").contains("PRIVATE"), "{bad}");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let api = Based::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}/u/PRIVATE"))
        .unwrap();
    let error = api.ping().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    let shown = shown(&error);
    assert!(!shown.contains("PRIVATE"), "{shown}");
    // Concord removed the top-level URL from `transport()` too (documented).
    let native = native(&error);
    assert!(
        !native.contains("PRIVATE"),
        "top-level URL removed: {native}"
    );
    assert!(error.transport().unwrap().url().is_none());
    // Positive control: the same request executed natively yields a reqwest error that shows it.
    let (http, request, _) = api.ping().prepare().unwrap().into_parts();
    let raw = http.execute(request).await.unwrap_err();
    assert!(format!("{raw} {raw:?}").contains("/u/PRIVATE"), "{raw:?}");
}

/// A user encoder whose description quotes the value (violating the `BodyError` contract).
pub struct Quoting(());

impl concord::Encode<String> for Quoting {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain");
    fn encode(value: String) -> Result<reqwest::Body, BodyError> {
        Err(BodyError::new(format!("cannot encode {value}")))
    }
}

/// A serde value whose serialization fails with a message containing data.
pub struct FailsWith(pub String);

impl serde::Serialize for FailsWith {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom(format!("refusing {}", self.0)))
    }
}

concord::api! {
    /// Encode-error fixtures.
    pub client Enc {
        base "http://enc.invalid";
        POST quoting(body b: String via Quoting);
        POST json(body b: FailsWith via Json);
    }
}

/// S09: a custom `BodyError` description is shown verbatim — the trust contract, not a guarantee.
#[test]
fn s09_custom_encoder_description_is_trusted_text() {
    let error = Enc::new(reqwest::Client::new())
        .quoting("S3CR3T-BODY")
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(
        error.to_string().contains("S3CR3T-BODY"),
        "documented boundary: {error}"
    );
}

/// Json encode error: positive control in `body_source`, negative in `Display`/`Debug`.
#[test]
fn json_encode_error_is_content_free() {
    let error = Enc::new(reqwest::Client::new())
        .json(FailsWith("S3CR3T-BODY".to_owned()))
        .prepare()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let shown = shown(&error);
    assert!(!shown.contains("S3CR3T"), "{shown}");
    assert!(
        error
            .body_source()
            .unwrap()
            .to_string()
            .contains("S3CR3T-BODY")
    );
    assert!(error.source().is_none());
}

/// S08: callers can classify without printing anything: kind, status, and the native transport
/// predicates. Encode-vs-construction is the one split only `Display` (static text) shows.
#[tokio::test]
async fn s08_classification_without_printing() {
    fn class(e: &concord::Error) -> &'static str {
        match e.kind() {
            ErrorKind::Build if e.body_source().is_some() => "encode (with source)",
            ErrorKind::Build => "build (construction, or an encode without source)",
            ErrorKind::Transport if e.transport().is_some_and(|t| t.is_connect()) => "connect",
            ErrorKind::Transport if e.transport().is_some_and(|t| t.is_timeout()) => "timeout",
            ErrorKind::Transport => "transport",
            ErrorKind::Status if e.status().is_some_and(|s| s.is_server_error()) => "5xx",
            ErrorKind::Status => "4xx",
            ErrorKind::Decode => "decode",
            _ => "other",
        }
    }
    let server = Server::reply(Reply::new(503)).await;
    let api = Based::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(class(&api.ping().send().await.unwrap_err()), "5xx");

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    let refused = Based::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    assert_eq!(class(&refused.ping().send().await.unwrap_err()), "connect");

    let e = Enc::new(reqwest::Client::new())
        .json(FailsWith("x".to_owned()))
        .prepare()
        .unwrap_err();
    assert_eq!(class(&e), "encode (with source)");
    let e = Enc::new(reqwest::Client::new())
        .quoting("x")
        .prepare()
        .unwrap_err();
    assert_eq!(
        class(&e),
        "build (construction, or an encode without source)"
    );
}

concord::api! {
    /// S03 fixture: remove + reintroduce under the same key.
    pub client Sticky(
        #[sensitive] header("x-k") k: String,
        #[sensitive] query("q") q: String,
    ) {
        base "http://sticky.invalid";
        GET reintroduced() {
            headers { remove "x-k"; "x-k" = "public-h"; }
            query { remove "q"; "q" = "public-q"; }
        }
    }
}

/// S03: removal and reintroduction keep the key sensitive (sticky). Positive control: the public
/// replacement is what the native request carries.
#[test]
fn s03_remove_and_reintroduce_stays_sensitive() {
    let p = Sticky::new(
        reqwest::Client::new(),
        StickyConfig {
            k: "S3CR3T-H".into(),
            q: "S3CR3T-Q".into(),
        },
    )
    .reintroduced()
    .prepare()
    .unwrap();
    assert_eq!(p.request().headers()["x-k"], "public-h");
    assert!(p.request().headers()["x-k"].is_sensitive());
    assert_eq!(p.request().url().query(), Some("q=public-q"));
    let shown = format!("{p:?}");
    assert!(
        shown.contains("x-k: <redacted>") && shown.contains("q=<redacted>"),
        "{shown}"
    );
    assert!(
        !shown.contains("public-") && !shown.contains("S3CR3T"),
        "{shown}"
    );
}
