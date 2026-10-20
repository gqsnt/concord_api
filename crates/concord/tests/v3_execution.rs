//! End-to-end execution of a downstream-generated v3 client against a
//! loopback server reached through an explicit proxy: explicit login and
//! storage, bearer placement, head-only absence, symbolic header/query
//! programs, pagination and the accepted-bytes terminal.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord::auth::{CredentialMaterial, SecretCredential};
use concord::http::SafeProxy;
use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{CallOptions, Json, SecretString, StoreOutcome, api};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
pub struct Login {
    pub user: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    pub id: String,
}

pub struct Token(SecretString);
impl CredentialMaterial for Token {}
impl SecretCredential for Token {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}
impl<'de> Deserialize<'de> for Token {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        SecretString::try_new(text)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

api! {
    client Exec {
        base "https://localhost/api/";
        request Json;
        response Json;

        values {
            query tenant: String;
            header("X-Locale") locale?: String;
        }

        credentials {
            credential session = endpoint login;
        }

        rate_limits {
            windows generous { allow 1000 per 1m; }
        }

        POST login(body input: Login) -> Token {
            id "exec.login";
            path "login";
        }

        scope secured() {
            auth { bearer session challenge unauthorized; }
            rate_limit "exec.reads" key [origin] using generous;

            GET item(id: String, locale_override?: String) -> Item {
                path "items/{id}";
                headers { when_present "X-Locale" = locale_override; }
                expose accepted_bytes;
            }

            GET find(id: String) -> Option<Item> {
                path "items/{id}";
                absent on 404;
            }

            GET list(query offset: u64 = 0, query count: NonZeroU32 = NonZeroU32::MIN) -> Vec<Item> {
                id "exec.list";
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }
        }
    }
}

struct Harness {
    server: TlsServer,
    _tunnel: ConnectProxy,
    client: Exec,
}

fn harness() -> Harness {
    let server = TlsServer::start().expect("tls server");
    let tunnel = ConnectProxy::start(server.address()).expect("tunnel");
    let proxy = SafeProxy::parse(tunnel.url()).expect("proxy");
    let certificate = server.certificate_pem();
    let client = Exec::builder()
        .set_tenant("acme")
        .set_locale("fr")
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(certificate)
        })
        .configure_runtime(|runtime| {
            runtime.rate_limiter(Arc::new(concord::rate_limit::LocalRateLimiter::new()))
        })
        .build()
        .expect("client");
    Harness {
        server,
        _tunnel: tunnel,
        client,
    }
}

fn queue_json(server: &TlsServer, body: &str) {
    server
        .queue_response_with_headers(
            200,
            &[("content-type", "application/json")],
            body.as_bytes().to_vec(),
        )
        .expect("queue");
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_storage_then_bearer_requests_follow_the_declaration() {
    let Harness {
        server,
        _tunnel,
        client,
    } = harness();
    queue_json(&server, "\"token-sentinel-1\"");
    queue_json(&server, "{\"id\":\"42\"}");
    server
        .queue_response(404, b"irrelevant missing body".to_vec())
        .expect("queue");
    queue_json(&server, "{\"id\":\"7\"}");

    let outcome = client
        .login(Login {
            user: "u".to_owned(),
        })
        .store_as_session()
        .await
        .expect("login");
    assert!(matches!(outcome, StoreOutcome::Installed { .. }));

    let item = client
        .secured()
        .item("42")
        .set_locale_override("de")
        .with_options(&CallOptions::new().cap_sends(3))
        .timeout(Duration::from_secs(5))
        .expect("timeout")
        .await
        .expect("item");
    assert_eq!(
        item,
        Item {
            id: "42".to_owned()
        }
    );

    let missing = client.secured().find("nope").await.expect("absent");
    assert!(missing.is_none());

    let bytes = client
        .secured()
        .item("7")
        .accepted_bytes()
        .await
        .expect("bytes");
    assert_eq!(bytes.value().as_ref(), b"{\"id\":\"7\"}");

    server
        .wait_for_requests(4, Duration::from_secs(5))
        .expect("requests");
    let requests = server.requests();
    assert_eq!(requests[0].method(), "POST");
    assert!(
        requests[0].target().ends_with("/api/login?tenant=acme"),
        "{}",
        requests[0].target()
    );
    assert_eq!(requests[0].header("x-locale"), Some("fr"));
    assert_eq!(requests[0].header("authorization"), None);
    assert!(
        requests[1].target().contains("/api/items/42?tenant=acme"),
        "{}",
        requests[1].target()
    );
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer token-sentinel-1")
    );
    assert_eq!(
        requests[1].header("x-locale"),
        Some("de"),
        "when_present replaces the inherited header"
    );
    assert_eq!(
        requests[3].header("x-locale"),
        Some("fr"),
        "absent override keeps the inherited header"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn offset_pagination_collects_pages_with_truthful_bounds() {
    let Harness {
        server,
        _tunnel,
        client,
    } = harness();
    queue_json(&server, "\"token\"");
    queue_json(&server, "[{\"id\":\"a\"}]");
    queue_json(&server, "[{\"id\":\"b\"}]");
    queue_json(&server, "[]");
    let outcome = client
        .login(Login {
            user: "u".to_owned(),
        })
        .store_as_session()
        .await
        .expect("login");
    assert!(matches!(outcome, StoreOutcome::Installed { .. }));
    let items = client
        .secured()
        .list()
        .collect_all(NonZeroU32::new(10).expect("nonzero"))
        .await
        .expect("collect");
    assert_eq!(
        items,
        [Item { id: "a".to_owned() }, Item { id: "b".to_owned() }]
    );
    let requests = server.requests();
    assert!(
        requests[1].target().contains("offset=0"),
        "{}",
        requests[1].target()
    );
    assert!(
        requests[2].target().contains("offset=1"),
        "{}",
        requests[2].target()
    );
}

#[test]
fn unbound_intents_bind_to_equivalent_plans() {
    let Harness { client, .. } = harness();
    let direct = client
        .secured()
        .item("9")
        .set_locale_override("it")
        .plan()
        .expect("direct");
    let intent = Exec::unbound()
        .secured()
        .item("9")
        .set_locale_override("it");
    let planned = client.plan(&intent).expect("unbound plan");
    assert_eq!(direct.observed_url(), planned.observed_url());
    let bound = client.request(intent).plan().expect("bound plan");
    assert_eq!(direct.observed_url(), bound.observed_url());
}
