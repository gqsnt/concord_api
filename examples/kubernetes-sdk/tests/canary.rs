//! Secrets: BearerToken (src/ids.rs:223) via KubeContext::with_bearer (src/context.rs:56) and,
//! separately, the declaration's `#[sensitive] authorization` header (src/api.rs:59) set on the
//! generated Kube with an unflagged HeaderValue; the `#[sensitive]` `continue` query value
//! (src/api.rs:69, ContinueToken src/ids.rs:192) sent from a generated list and from a resumed
//! ListPager Continuation; a response `metadata.continue` token (redacted in Continuation and
//! Status Debug, src/list.rs:121, src/model/meta.rs:130) that the pager then sends; the withheld
//! Status `message` (src/model/meta.rs:130) and ConfigMap data/binaryData values
//! (src/model/resources.rs:241). Read-only SDK (no writes): a namespaced pod list, the chunked
//! pager, a ConfigMap get and a pod watch (generated send() and watch::open) cover all seven
//! scenarios. Wire exemptions: the Status-message and ConfigMap-data canaries are response-only
//! (the server sends them, no request carries them); each is shown to reach the decoder instead.
//! Excludes native reqwest values, ContinueToken Display/expose() and Continuation::token() (the
//! raw token by design), Status::message and ConfigMap::data read as fields, and
//! body_unredacted/source_unredacted.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroU32, time::Duration};

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
    concord_example_testserver::refused_url()
}

fn reply(case: usize, success: &str, forbidden: &str, internal: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(403, forbidden),
        2 => Reply::json(500, internal),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
            format!("{{\"incomplete\":\"{echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error<D: 'static>(c: &mut Canaries, e: &concord::Error<D>) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

async fn receive<T: std::fmt::Debug, D: 'static>(
    c: &mut Canaries,
    prepared: concord::Prepared<T, D>,
) {
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

use concord::jsonl::JsonValuesReader;
use concord::reqwest::header::HeaderValue;
use concord_example_kubernetes_sdk::list::{Continuation, ListError, ListPager};
use concord_example_kubernetes_sdk::model::Pod;
use concord_example_kubernetes_sdk::watch::{self, WatchEvent, WatchOpenError, WatchStream};
use concord_example_kubernetes_sdk::{
    ApiError, BearerToken, ContinueToken, Kube, KubeContext, KubeError, NamespaceName, ObjectName,
    ResourceVersion,
};

const PODS: &str = "Kube.core_v1.namespaced.pods";

fn api_error(c: &mut Canaries, e: &ApiError) {
    c.assert_debug_absent("ApiError", e);
    c.assert_display_absent("ApiError", e);
    c.assert_error_absent("ApiError/source chain", e);
    c.assert_debug_absent("Status document", &e.status());
}

/// Classifies a failed generated send through the SDK, checking both renderings.
async fn kube_error(
    c: &mut Canaries,
    e: concord::Error<concord_example_kubernetes_sdk::Status>,
) -> KubeError {
    concord_error(c, &e);
    let e = KubeError::from_concord(e).await;
    c.assert_debug_absent("KubeError", &e);
    c.assert_error_absent("KubeError/source chain", &e);
    match &e {
        KubeError::Request(e) => concord_error(c, e),
        KubeError::Api(a) => api_error(c, a),
        _ => {}
    }
    e
}

fn list_error(c: &mut Canaries, e: &ListError) {
    c.assert_debug_absent("ListError", e);
    c.assert_error_absent("ListError/source chain", e);
    match e {
        ListError::Request(e) => concord_error(c, e),
        ListError::Api(a) | ListError::Expired(a) => api_error(c, a),
        _ => {}
    }
}

async fn drain(c: &mut Canaries, mut events: WatchStream<Pod>) -> Vec<WatchEvent<Pod>> {
    c.assert_debug_absent("watch stream", &events);
    let mut out = Vec::new();
    while let Some(item) = events.next().await {
        c.assert_debug_absent("watch item", &item);
        match item {
            Ok(event) => out.push(event),
            Err(e) => {
                c.assert_error_absent("watch ReadError", &e);
                c.assert_debug_absent("watch ReadError kind", &e.kind());
                assert_eq!(e.operation(), Some("Kube.core_v1.namespaced.watch_pods"));
            }
        }
    }
    c.assert_debug_absent("watch completion", &events.completion());
    c.assert_debug_absent("watch position", &events.position());
    c.assert_debug_absent("finished watch stream", &events);
    out
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("kubernetes");
    let token = c.issue();
    let raw_token = c.issue();
    let cont_in = c.issue();
    let cont_out = c.issue();
    let message = c.issue();
    let config_data = c.issue();
    let bearer = BearerToken::new(&token);
    let resume = Continuation::resume(
        ContinueToken::from_server(&cont_in),
        ResourceVersion::new("100"),
    );
    c.assert_debug_absent("bearer token", &bearer);
    c.assert_debug_absent("continue token", &ContinueToken::from_server(&cont_in));
    c.assert_debug_absent("resumed continuation", &resume);
    for case in 0..7 {
        let (echo, next, msg, data) = (
            token.clone(),
            cont_out.clone(),
            message.clone(),
            config_data.clone(),
        );
        let server = TestServer::start(move |r: &Recorded| {
            let query = r.query().unwrap_or("");
            let pod = r#"{"metadata":{"name":"web-1","namespace":"team-a","resourceVersion":"99"}}"#;
            let status = |code: u16, reason: &str| {
                format!(
                    r#"{{"kind":"Status","apiVersion":"v1","metadata":{{"continue":"{next}"}},"status":"Failure","message":"{msg} {echo}","reason":"{reason}","code":{code}}}"#
                )
            };
            let success = if query.contains("watch=1") {
                format!(
                    "{{\"type\":\"ADDED\",\"object\":{pod}}}\n\
                     {{\"type\":\"BOOKMARK\",\"object\":{{\"metadata\":{{\"resourceVersion\":\"101\"}}}}}}\n\
                     {{\"type\":\"ERROR\",\"object\":{}}}\n",
                    status(410, "Expired")
                )
            } else if r.path().contains("/configmaps/") {
                format!(
                    r#"{{"kind":"ConfigMap","apiVersion":"v1","metadata":{{"name":"app-config"}},"data":{{"password":"{data}"}},"binaryData":{{"key":"{data}"}}}}"#
                )
            } else if query.contains(&format!("continue={next}")) {
                format!(r#"{{"kind":"PodList","metadata":{{"resourceVersion":"100"}},"items":[{pod}]}}"#)
            } else {
                format!(
                    r#"{{"kind":"PodList","metadata":{{"resourceVersion":"100","continue":"{next}","remainingItemCount":1}},"items":[{pod}]}}"#
                )
            };
            reply(case, &success, &status(403, "Forbidden"), &status(500, "InternalError"), &echo)
        })
        .await;
        let url = if case == 6 { refused() } else { server.url() };
        let kube = KubeContext::new(http())
            .with_base_url(&url)
            .unwrap()
            .with_bearer(&bearer)
            .unwrap();
        let mut raw_header = HeaderValue::from_str(&format!("Bearer {raw_token}")).unwrap();
        raw_header.set_sensitive(false);
        let raw = concord::BaseUrl::parse(url.as_ref())
            .map(|origin| {
                Kube::new(
                    http(),
                    concord_example_kubernetes_sdk::KubeConfig { origin },
                )
            })
            .unwrap()
            .set_authorization(raw_header);
        let ns = kube.namespace(NamespaceName::new("team-a").unwrap());
        c.assert_debug_absent("context", &kube);
        c.assert_debug_absent("generated client", kube.client());
        c.assert_debug_absent("generated client (raw header)", &raw);
        c.assert_debug_absent("core scope", &kube.core());
        c.assert_debug_absent("apps scope", &kube.apps());
        c.assert_debug_absent("batch scope", &kube.batch());
        c.assert_debug_absent("networking scope", &kube.networking());
        c.assert_debug_absent("namespace context", &ns);
        c.assert_debug_absent("namespaced core scope", ns.core());
        c.assert_debug_absent("namespaced apps scope", ns.apps());

        // Read: a generated list carrying the request continue token.
        let op = ns
            .core()
            .pods()
            .set_limit(1)
            .set_continue_token(ContinueToken::from_server(&cont_in));
        c.assert_debug_absent("list endpoint", &op);
        receive(&mut c, op.clone().prepare().unwrap()).await;
        let result = op.send().await;
        c.assert_debug_absent("list Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(list) => {
                c.assert_debug_absent("pod list", &list);
                c.assert_debug_absent("list metadata", &list.metadata);
                assert_eq!(list.metadata.next_token().unwrap().expose(), cont_out);
            }
            Err(e) => {
                let e = kube_error(&mut c, e).await;
                if case == 1 {
                    // Safe diagnosis survives: endpoint, status and reason, not the message.
                    let shown = e.to_string();
                    assert!(
                        shown.contains(PODS)
                            && shown.contains("403")
                            && shown.contains("Forbidden"),
                        "{shown}"
                    );
                    let api = e.api().unwrap();
                    assert_eq!(api.http_status().as_u16(), 403);
                    assert_eq!(api.endpoint(), Some(PODS));
                    assert_eq!(api.reason(), Some("Forbidden"));
                    assert!(api.offers_inconsistent_continue());
                    let withheld = api.status().unwrap().message.as_deref().unwrap();
                    assert!(
                        withheld.contains(&message),
                        "the Status message reached the decoder"
                    );
                }
            }
        }
        let result = raw.core_v1().pods().send().await;
        c.assert_debug_absent("raw-client list Result", &result);
        if let Err(e) = result {
            kube_error(&mut c, e).await;
        }

        // The chunked pager: a resumed key (request token), then the server's next key.
        let pager = ListPager::new(ns.core().pods(), NonZeroU32::new(1).unwrap());
        c.assert_debug_absent("list pager", &pager);
        let page = pager.fetch(Some(resume.clone())).await;
        c.assert_debug_absent("page Result", &page);
        assert_eq!(page.is_ok(), case == 0);
        match page {
            Ok(page) => {
                c.assert_debug_absent("page", &page);
                c.assert_debug_absent("next key", &page.next);
                c.assert_debug_absent("page metadata", &page.meta);
                let next = page.next.unwrap();
                assert_eq!(next.token().expose(), cont_out);
                let last = pager.fetch(Some(next)).await;
                c.assert_debug_absent("last page Result", &last);
                assert!(last.unwrap().next.is_none());
            }
            Err(e) => list_error(&mut c, &e),
        }

        // A ConfigMap get: data values are withheld response content.
        let op = ns.core().config_map(ObjectName::new("app-config").unwrap());
        c.assert_debug_absent("config map endpoint", &op);
        receive(&mut c, op.clone().prepare().unwrap()).await;
        let result = op.send().await;
        c.assert_debug_absent("config map Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(map) => {
                c.assert_debug_absent("config map", &map);
                c.assert_debug_absent("config map metadata", &map.metadata);
                assert_eq!(
                    map.data["password"], config_data,
                    "data reached the decoder"
                );
                assert_eq!(map.binary_data["key"], config_data);
            }
            Err(e) => drop(kube_error(&mut c, e).await),
        }

        // A pod watch, through the generated send() and through watch::open.
        let op = ns
            .core()
            .watch_pods()
            .set_resource_version(ResourceVersion::new("99"));
        c.assert_debug_absent("watch endpoint", &op);
        c.assert_debug_absent("watch Prepared", &op.clone().prepare().unwrap());
        let opened = op.clone().send().await;
        c.assert_debug_absent("watch send Result", &opened);
        assert_eq!(opened.is_ok(), matches!(case, 0 | 3 | 5));
        match opened {
            Ok(events) => {
                let events = drain(&mut c, events).await;
                c.assert_debug_absent("watch events", &events);
                assert_eq!(events.len(), if case == 0 { 3 } else { 0 });
                if let Some(WatchEvent::Error(status)) = events.last() {
                    assert!(status.is_expired());
                    assert!(status.message.as_deref().unwrap().contains(&message));
                }
            }
            Err(e) => drop(kube_error(&mut c, e).await),
        }
        let opened = watch::open(op, &JsonValuesReader::default()).await;
        c.assert_debug_absent("watch open Result", &opened);
        match opened {
            Ok(events) => drop(drain(&mut c, events).await),
            Err(e) => {
                c.assert_debug_absent("WatchOpenError", &e);
                c.assert_error_absent("WatchOpenError/source chain", &e);
                match &e {
                    WatchOpenError::Request(e) => concord_error(&mut c, e),
                    WatchOpenError::Api(a) => api_error(&mut c, a),
                    WatchOpenError::Refused(r) => c.assert_error_absent("OpenError", r),
                }
            }
        }

        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire_except(&requests, &[&message, &config_data]);
            assert!(requests.iter().all(|r| r.header("authorization")
                == Some(format!("Bearer {token}").as_str())
                || r.header("authorization") == Some(format!("Bearer {raw_token}").as_str())));
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let e = KubeContext::new(http())
        .with_bearer(&BearerToken::new(format!("{token}\n")))
        .unwrap_err();
    c.assert_debug_absent("local token error", &e);
    c.assert_error_absent("local token error", &e);
    assert!(c.checked() >= 300);
    println!(
        "kubernetes: {} renderings; positive control passed (Status message and ConfigMap data are response-only)",
        c.checked()
    );
}
