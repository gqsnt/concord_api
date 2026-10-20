//! Generated endpoint opt-in, wire validators and bounded shared entity storage.

use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use concord::error::{DispatchKnowledge, ErrorCategory, FailureStage};
use concord::http::{SafeProxy, StatusCode};
use concord::observation::{OperationEvent, OperationObserver};
use concord::response::{
    Buffered, ResponseBodyError, ResponseBodyErrorKind, ResponseHead, ResponseRepresentation,
};
use concord::{ApiError, Binary, Bytes, CallOptions, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

#[derive(Debug, Eq, PartialEq)]
pub struct Decoded {
    body: Bytes,
    status: StatusCode,
}

pub struct Inspect;

impl ResponseRepresentation<Decoded> for Inspect {
    type Driver = Buffered;
    const ACCEPT: Option<&'static str> = Some("application/octet-stream");

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        if head
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            != Some("application/octet-stream")
        {
            return Err(ResponseBodyError::new(
                ResponseBodyErrorKind::InvalidMediaType,
            ));
        }
        Ok(())
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Decoded, ResponseBodyError> {
        Ok(Decoded {
            body,
            status: head.status,
        })
    }
}

api! {
    client EtagApi {
        base "http://etag.example.test/";
        timeout 5s;

        GET inspect() -> Decoded via Inspect {
            path "inspect";
            status 200;
            revalidate etag;
        }
        GET accepts_not_modified() -> Bytes via Binary {
            path "permissive";
            status [200, 304];
            revalidate etag;
        }
        GET item(query key: String, header("X-View") view: String) -> Bytes via Binary {
            path "item";
            revalidate etag;
        }
        GET twin(query key: String, header("X-View") view: String) -> Bytes via Binary {
            path "item";
            revalidate etag;
        }
        GET plain() -> Bytes via Binary { path "plain"; }
    }
}

fn client(server: &ScriptedServer) -> EtagApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    EtagApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("generated ETag client")
}

fn limited_client(server: &ScriptedServer, entries: usize, bytes: usize) -> EtagApi {
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    EtagApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.revalidation_cache(entries, bytes))
        .build()
        .expect("bounded generated ETag client")
}

fn tagged(body: &str, tag: &str) -> ResponseScript {
    ResponseScript::ok(body.as_bytes().to_vec())
        .with_header("content-type", "application/octet-stream")
        .with_header("etag", tag)
}

fn not_modified() -> ResponseScript {
    // Deliberately omit content-type: the cached representation head must be used.
    ResponseScript::status(304, Vec::new())
}

fn assert_validators(server: &ScriptedServer, expected: &[Option<&str>]) {
    let requests = server.requests();
    assert_eq!(requests.len(), expected.len());
    for (index, (request, expected)) in requests.iter().zip(expected).enumerate() {
        assert_eq!(request.method(), "GET");
        assert_eq!(
            request.header("if-none-match"),
            *expected,
            "request {index}"
        );
    }
}

fn assert_missing_entity(error: &ApiError) {
    assert_eq!(error.category(), ErrorCategory::ResponseContract);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::NOT_MODIFIED));
}

#[tokio::test]
async fn strong_and_weak_validators_are_exact_and_clones_decode_cached_304_as_200() {
    let endpoint = EtagApi::contract()
        .endpoint("etag_api.inspect")
        .expect("inspected endpoint");
    assert!(endpoint.accepted().contains(200));
    assert!(!endpoint.accepted().contains(304));

    for tag in ["\"opaque-strong\"", "W/\"opaque-weak\""] {
        let server = ScriptedServer::start([
            tagged("retained representation", tag),
            not_modified(),
            not_modified(),
        ])
        .expect("ETag server");
        let client = client(&server);
        let cloned = client.clone();
        let original = client.inspect().response().await.expect("initial entity");
        assert_eq!(original.status(), StatusCode::OK);

        let revalidated = cloned.inspect().response().await.expect("cached entity");
        assert_eq!(revalidated.status(), StatusCode::OK);
        assert_eq!(revalidated.value().status, StatusCode::OK);
        assert_eq!(revalidated.value().body, "retained representation");
        assert_eq!(revalidated.metadata().attempts_started(), 1);
        assert_eq!(revalidated.metadata().policy_retries(), 0);

        let direct = client.inspect().await.expect("ordinary typed await");
        assert_eq!(direct, revalidated.into_value());
        assert_validators(&server, &[None, Some(tag), Some(tag)]);
        assert_eq!(
            server.requests()[0].header("accept"),
            Some("application/octet-stream")
        );
    }
}

#[tokio::test]
async fn cached_bodies_obey_each_calls_response_byte_cap_without_invalidating_the_entity() {
    for chunked in [false, true] {
        let seed = if chunked {
            ResponseScript::Chunked {
                status: 200,
                headers: vec![
                    ("content-type".into(), "application/octet-stream".into()),
                    ("etag".into(), "\"retained\"".into()),
                ],
                chunks: vec![b"reta".to_vec(), b"ined".to_vec()],
            }
        } else {
            tagged("retained", "\"retained\"")
        };
        let server = ScriptedServer::start([seed, not_modified(), not_modified()])
            .expect("cached response cap server");
        let client = client(&server);
        assert_eq!(
            client.inspect().await.expect("seed entity").body,
            "retained"
        );

        let cap = CallOptions::new()
            .cap_response_bytes(NonZeroUsize::new("retained".len() - 1).expect("positive cap"));
        let error = client
            .inspect()
            .with_options(&cap)
            .await
            .expect_err("the retained body exceeds this call's cap");
        assert_eq!(error.category(), ErrorCategory::ResponseBody);
        assert_eq!(error.failure_stage(), FailureStage::ResponseAcquisition);
        assert_eq!(
            error.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );
        assert_eq!(error.last_response_status(), Some(StatusCode::NOT_MODIFIED));

        let response = client
            .inspect()
            .response()
            .await
            .expect("uncapped cached entity");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.value().body, "retained");
        assert_eq!(response.metadata().attempts_started(), 1);
        assert_validators(&server, &[None, Some("\"retained\""), Some("\"retained\"")]);
    }
}

#[derive(Default)]
struct NotModifiedGate {
    reached: tokio::sync::Notify,
    released: Mutex<bool>,
    wake: Condvar,
}

impl NotModifiedGate {
    fn release(&self) {
        *self.released.lock().expect("304 gate") = true;
        self.wake.notify_all();
    }
}

impl OperationObserver for NotModifiedGate {
    fn observe(&self, event: OperationEvent<'_>) {
        if matches!(event, OperationEvent::ResponseHeadObserved(head) if head.status == StatusCode::NOT_MODIFIED)
        {
            // Hold this attempt after the real 304 head is recorded and before
            // cache lookup. The other runtime worker can complete an eviction.
            let released = self.released.lock().expect("304 gate");
            self.reached.notify_one();
            let (released, _) = self
                .wake
                .wait_timeout_while(released, Duration::from_secs(3), |released| !*released)
                .expect("304 gate wait");
            assert!(*released, "the test must release its paused 304");
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eviction_before_a_conditional_304_is_processed_causes_a_contract_failure() {
    let server = ScriptedServer::start([
        tagged("original-a", "\"a\""),
        not_modified(),
        tagged("other-b", "\"b\""),
        tagged("fresh-a", "\"a2\""),
    ])
    .expect("in-flight eviction server");
    let gate = Arc::new(NotModifiedGate::default());
    let observer = Arc::clone(&gate);
    let proxy = SafeProxy::parse(server.url("")).expect("loopback proxy");
    let client = EtagApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| {
            runtime
                .revalidation_cache(1, 1024)
                .operation_observer(observer)
        })
        .build()
        .expect("single-entry client with an observed 304 gate");
    assert_eq!(client.item("a", "en").await.expect("seed A"), "original-a");

    let cloned = client.clone();
    let pending = tokio::spawn(async move { cloned.item("a", "en").await });
    tokio::time::timeout(Duration::from_secs(2), gate.reached.notified())
        .await
        .expect("the conditional attempt reached its 304 head");
    let replacement = client.item("b", "en").await;
    gate.release();
    assert_eq!(
        replacement.expect("B evicts A before releasing the 304"),
        "other-b"
    );

    let error = pending
        .await
        .expect("conditional attempt task")
        .expect_err("A is no longer retained when its 304 is processed");
    assert_missing_entity(&error);
    assert_eq!(
        client.item("a", "en").await.expect("fresh A request"),
        "fresh-a"
    );
    assert_validators(&server, &[None, Some("\"a\""), None, None]);
}

#[tokio::test]
async fn new_200_replaces_both_the_body_and_validator() {
    let server = ScriptedServer::start([
        tagged("old", "\"v1\""),
        tagged("replacement", "W/\"v2\""),
        not_modified(),
    ])
    .expect("replacement server");
    let client = client(&server);
    assert_eq!(client.inspect().await.expect("initial entity").body, "old");
    assert_eq!(
        client.inspect().await.expect("new entity").body,
        "replacement"
    );
    assert_eq!(
        client.inspect().await.expect("new cached entity").body,
        "replacement"
    );
    assert_validators(&server, &[None, Some("\"v1\""), Some("W/\"v2\"")]);
}

#[tokio::test]
async fn accepted_responses_without_a_tag_or_with_no_store_invalidate_an_old_entity() {
    for no_store in [false, true] {
        let replacement = if no_store {
            tagged("uncacheable", "\"must-not-retain\"")
                .with_header("cache-control", "private, no-store")
        } else {
            ResponseScript::ok(b"uncacheable".to_vec())
                .with_header("content-type", "application/octet-stream")
        };
        let server = ScriptedServer::start([
            tagged("old", "\"old\""),
            replacement,
            tagged("fresh", "\"fresh\""),
            not_modified(),
        ])
        .expect("invalidation server");
        let client = client(&server);
        assert_eq!(client.inspect().await.expect("initial entity").body, "old");
        assert_eq!(
            client
                .inspect()
                .await
                .expect("uncacheable replacement")
                .body,
            "uncacheable"
        );
        assert_eq!(client.inspect().await.expect("fresh entity").body, "fresh");
        assert_eq!(
            client.inspect().await.expect("fresh cached entity").body,
            "fresh"
        );
        assert_validators(&server, &[None, Some("\"old\""), None, Some("\"fresh\"")]);
    }
}

#[tokio::test]
async fn an_unmatched_304_is_a_contract_failure_even_when_304_is_accepted() {
    let server =
        ScriptedServer::start([not_modified(), not_modified()]).expect("unmatched 304 server");
    let client = client(&server);
    let strict = client.inspect().await.expect_err("no retained entity");
    assert_missing_entity(&strict);
    let permissive = client
        .accepts_not_modified()
        .await
        .expect_err("304 cannot become an empty payload");
    assert_missing_entity(&permissive);
    assert_validators(&server, &[None, None]);
}

#[tokio::test]
async fn zero_in_either_storage_limit_disables_revalidation_storage() {
    for (entries, bytes) in [(0, 1024), (8, 0)] {
        let server =
            ScriptedServer::start([tagged("not retained", "\"disabled\""), not_modified()])
                .expect("disabled storage server");
        let client = limited_client(&server, entries, bytes);
        assert_eq!(
            client.inspect().await.expect("ordinary response").body,
            "not retained"
        );
        assert_missing_entity(&client.inspect().await.expect_err("storage is disabled"));
        assert_validators(&server, &[None, None]);
    }
}

#[tokio::test]
async fn entry_and_byte_limits_evict_the_least_recently_used_entity() {
    for (entries, bytes) in [(2, 1024), (16, 6)] {
        let server = ScriptedServer::start([
            tagged("aaa", "\"a\""),
            tagged("bbb", "\"b\""),
            not_modified(),
            tagged("ccc", "\"c\""),
            not_modified(),
            tagged("new-b", "\"b2\""),
        ])
        .expect("LRU server");
        let client = limited_client(&server, entries, bytes);
        for (key, expected) in [
            ("a", "aaa"),
            ("b", "bbb"),
            ("a", "aaa"),
            ("c", "ccc"),
            ("a", "aaa"),
            ("b", "new-b"),
        ] {
            assert_eq!(
                client.item(key, "en").await.expect("LRU response"),
                expected
            );
        }
        assert_validators(
            &server,
            &[None, None, Some("\"a\""), None, Some("\"a\""), None],
        );
    }
}

#[tokio::test]
async fn an_entity_larger_than_the_byte_limit_is_delivered_without_being_retained() {
    let server = ScriptedServer::start([
        tagged("1234567", "\"oversized\""),
        tagged("small", "\"fits\""),
        not_modified(),
    ])
    .expect("oversized entity server");
    let client = limited_client(&server, 16, 6);
    assert_eq!(
        client
            .inspect()
            .await
            .expect("oversized response still decodes")
            .body,
        "1234567"
    );
    assert_eq!(
        client.inspect().await.expect("smaller response").body,
        "small"
    );
    assert_eq!(
        client
            .inspect()
            .await
            .expect("smaller cached response")
            .body,
        "small"
    );
    assert_validators(&server, &[None, None, Some("\"fits\"")]);
}

#[tokio::test]
async fn endpoint_query_and_public_headers_partition_the_entity_identity() {
    let server = ScriptedServer::start([
        tagged("original", "\"original\""),
        tagged("query", "\"query\""),
        tagged("header", "\"header\""),
        tagged("endpoint", "\"endpoint\""),
        not_modified(),
        not_modified(),
        not_modified(),
        not_modified(),
    ])
    .expect("request identity server");
    let client = client(&server);
    for _ in 0..2 {
        assert_eq!(
            client.item("a", "en").await.expect("original key"),
            "original"
        );
        assert_eq!(client.item("b", "en").await.expect("query key"), "query");
        assert_eq!(client.item("a", "fr").await.expect("header key"), "header");
        assert_eq!(
            client.twin("a", "en").await.expect("endpoint key"),
            "endpoint"
        );
    }
    assert_validators(
        &server,
        &[
            None,
            None,
            None,
            None,
            Some("\"original\""),
            Some("\"query\""),
            Some("\"header\""),
            Some("\"endpoint\""),
        ],
    );
    let requests = server.requests();
    assert_ne!(requests[0].target(), requests[1].target());
    assert_eq!(requests[0].target(), requests[2].target());
    assert_eq!(requests[0].target(), requests[3].target());
    assert_eq!(requests[0].header("x-view"), requests[3].header("x-view"));
}

#[tokio::test]
async fn independent_client_builds_keep_separate_stores() {
    let server = ScriptedServer::start([
        tagged("first client", "\"first\""),
        tagged("second client", "\"second\""),
        not_modified(),
        not_modified(),
    ])
    .expect("independent clients server");
    let first = client(&server);
    let second = client(&server);
    assert_eq!(
        first.inspect().await.expect("first client seed").body,
        "first client"
    );
    assert_eq!(
        second.inspect().await.expect("second client seed").body,
        "second client"
    );
    assert_eq!(
        first
            .inspect()
            .await
            .expect("first client's cached entity")
            .body,
        "first client"
    );
    assert_eq!(
        second
            .inspect()
            .await
            .expect("second client's cached entity")
            .body,
        "second client"
    );
    assert_validators(
        &server,
        &[None, None, Some("\"first\""), Some("\"second\"")],
    );
}

#[tokio::test]
async fn an_endpoint_without_opt_in_does_not_retain_or_send_a_validator() {
    let server = ScriptedServer::start([
        tagged("first", "\"unused\""),
        tagged("second", "\"unused\""),
    ])
    .expect("non-revalidating endpoint server");
    let client = client(&server);
    assert_eq!(
        client.plain().await.expect("first ordinary request"),
        "first"
    );
    assert_eq!(
        client.plain().await.expect("second ordinary request"),
        "second"
    );
    assert_validators(&server, &[None, None]);
}

#[cfg(feature = "default-tls")]
mod authenticated {
    use super::*;
    use concord::SecretString;
    use concord::auth::AccessToken;
    use concord_test_support::proxy::ConnectProxy;
    use concord_test_support::tls::TlsServer;

    api! {
        client AuthEtagApi {
            base "https://localhost/";
            timeout 5s;
            credentials {
                secret token;
                credential access = bearer(secret.token);
            }

            GET bearer() -> Bytes via Binary {
                path "resource";
                auth { bearer access; }
                revalidate etag;
            }
            GET header() -> Bytes via Binary {
                path "resource";
                auth { header "X-Token" = access; }
                revalidate etag;
            }
            GET query() -> Bytes via Binary {
                path "resource";
                auth { query "token" = access; }
                revalidate etag;
            }
        }
    }

    async fn fetch(client: &AuthEtagApi, placement: &str) -> Result<Bytes, ApiError> {
        match placement {
            "bearer" => client.bearer().await,
            "header" => client.header().await,
            "query" => client.query().await,
            _ => unreachable!("fixed test placements"),
        }
    }

    #[tokio::test]
    async fn resolved_credentials_partition_bearer_header_and_query_keys_across_shared_clones() {
        for placement in ["bearer", "header", "query"] {
            let server = TlsServer::start().expect("verified TLS server");
            let tunnel = ConnectProxy::start(server.address()).expect("CONNECT tunnel");
            for (status, body, tag) in [
                (200, "alice response", Some("\"alice\"")),
                (304, "", None),
                (200, "bob response", Some("\"bob\"")),
                (304, "", None),
                (304, "", None),
            ] {
                let headers: Vec<_> = tag.map(|tag| ("etag", tag)).into_iter().collect();
                server
                    .queue_response_with_headers(status, &headers, body.as_bytes().to_vec())
                    .expect("authenticated response script");
            }
            let proxy = SafeProxy::parse(tunnel.url()).expect("loopback TLS proxy");
            let certificate = server.certificate_pem();
            let client = AuthEtagApi::builder()
                .token(SecretString::try_new("alice-token").expect("test secret"))
                .configure_http(move |http| {
                    http.http1_only()
                        .proxy(proxy)
                        .add_root_certificate_pem(certificate)
                })
                .build()
                .expect("authenticated generated ETag client");
            let cloned = client.clone();
            assert_eq!(
                fetch(&client, placement).await.expect("Alice seed"),
                "alice response"
            );
            assert_eq!(
                fetch(&cloned, placement)
                    .await
                    .expect("clone shares Alice entity"),
                "alice response"
            );

            cloned
                .credentials()
                .replace_access(AccessToken::try_new("bob-token").expect("Bob token"))
                .expect("replace credential through clone");
            assert_eq!(
                fetch(&client, placement)
                    .await
                    .expect("Bob cannot reuse Alice entity"),
                "bob response"
            );
            assert_eq!(
                fetch(&cloned, placement).await.expect("Bob cached entity"),
                "bob response"
            );

            client
                .credentials()
                .replace_access(AccessToken::try_new("alice-token").expect("Alice token"))
                .expect("restore original material");
            assert_eq!(
                fetch(&cloned, placement)
                    .await
                    .expect("restored material selects Alice entity"),
                "alice response"
            );
            let requests = server.requests();
            assert_eq!(requests.len(), 5);
            for (request, validator) in requests.iter().zip([
                None,
                Some("\"alice\""),
                None,
                Some("\"bob\""),
                Some("\"alice\""),
            ]) {
                assert_eq!(request.header("if-none-match"), validator, "{placement}");
            }
            match placement {
                "bearer" => {
                    assert_eq!(
                        requests[0].header("authorization"),
                        Some("Bearer alice-token")
                    );
                    assert_eq!(
                        requests[2].header("authorization"),
                        Some("Bearer bob-token")
                    );
                }
                "header" => {
                    assert_eq!(requests[0].header("x-token"), Some("alice-token"));
                    assert_eq!(requests[2].header("x-token"), Some("bob-token"));
                }
                "query" => {
                    assert!(requests[0].target().ends_with("?token=alice-token"));
                    assert!(requests[2].target().ends_with("?token=bob-token"));
                }
                _ => unreachable!("fixed test placements"),
            }
        }
    }
}
