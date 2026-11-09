//! F3 regressions: a followed endpoint is sent through its client's own send policy.
use concord::{Endpoint, Follow, SendOperation, UrlContinuation, reqwest};
use concord_sdk_digitalocean::{
    DigitalOcean, DigitalOceanDropletsList, Error, HttpClass, Policy, Response,
};
use concord_testserver::{Recorded, Reply, TestServer};
use serde::Deserialize;
use std::sync::{Arc, Mutex};

const CANARY: &str = "F3_SYNTHETIC_CURSOR_9284";

#[tokio::test]
async fn followed_sdk_operation_keeps_provider_error_policy_and_identity() {
    let server = TestServer::start(|request: &Recorded| {
        if request.target == "/v2/droplets" {
            Reply::json(
                200,
                format!(
                    r#"{{"droplets":[],"links":{{"pages":{{"next":"http://{}/v2/droplets?b=2&a=1&cursor={CANARY}"}}}}}}"#,
                    request.host,
                ),
            )
        } else {
            assert_eq!(
                request.target,
                format!("/v2/droplets?b=2&a=1&cursor={CANARY}")
            );
            Reply::json(429, r#"{"id":"too_many_requests","message":"later"}"#)
                .header("retry-after", "3")
        }
    })
    .await;
    let api = DigitalOcean::for_fixture("SYNTHETIC_TOKEN", &server.url()).unwrap();
    let endpoint = api.droplets().list();
    let first = endpoint.clone().send().await.unwrap();
    let next = DigitalOceanDropletsList::next_url(&first.value)
        .unwrap()
        .unwrap();
    assert!(!format!("{next:?} {next}").contains(CANARY));
    let followed = endpoint.follow(next);
    assert!(!format!("{followed:?} {:?}", followed.clone().prepare().unwrap()).contains(CANARY));
    assert_eq!(
        server.requests().len(),
        1,
        "construction and prepare send nothing"
    );
    let error = followed.send().await.unwrap_err();
    assert!(!format!("{error} {error:?}").contains(CANARY));
    let Error::Provider(provider) = error else {
        panic!("original SDK policy was bypassed")
    };
    assert_eq!(provider.class(), HttpClass::RateLimited);
    assert!(provider.document.is_ok());
    assert_eq!(provider.meta.retry_after_seconds().unwrap(), Some(3));
    assert_eq!(
        provider.meta.operation().unwrap(),
        DigitalOceanDropletsList::operation()
    );
    let wire = server.requests();
    assert_eq!(wire.len(), 2, "no retries");
    assert_eq!(
        wire[1].header("authorization"),
        Some("Bearer SYNTHETIC_TOKEN")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn followed_url_on_another_origin_is_refused_unsent() {
    // A live foreign origin: if the origin check were missing, it would receive the request.
    let foreign = TestServer::start(|_: &Recorded| Reply::json(200, r#"{"droplets":[]}"#)).await;
    let foreign_origin = foreign.url();
    let server = TestServer::start(move |_: &Recorded| {
        Reply::json(
            200,
            format!(
                r#"{{"droplets":[],"links":{{"pages":{{"next":"{foreign_origin}/v2/droplets?cursor={CANARY}"}}}}}}"#,
            ),
        )
    })
    .await;
    let api = DigitalOcean::for_fixture("SYNTHETIC_TOKEN", &server.url()).unwrap();
    let endpoint = api.droplets().list();
    let first = endpoint.clone().send().await.unwrap();
    let next = DigitalOceanDropletsList::next_url(&first.value)
        .unwrap()
        .unwrap();
    // Sent directly, without the SDK's own validator: Concord's origin check refuses it.
    let error = endpoint.follow(next).send().await.unwrap_err();
    assert!(!format!("{error} {error:?}").contains(CANARY));
    let Error::Concord(error) = error else {
        panic!("expected Concord's origin refusal, got {error:?}")
    };
    assert_eq!(error.kind(), concord::ErrorKind::Build);
    assert!(format!("{error}").contains("another origin"), "{error}");
    assert!(
        foreign.requests().is_empty(),
        "the foreign origin received a request"
    );
    assert_eq!(server.requests().len(), 1, "only the first page was sent");
    server.shutdown().await;
    foreign.shutdown().await;
}

/// A page whose next URL is a body field.
#[derive(Deserialize)]
pub struct ProbePage {
    items: Vec<u64>,
    next: Option<String>,
}

/// Records each operation it sends, then sends through the SDK policy.
#[derive(Clone)]
pub struct CountingPolicy(Arc<Mutex<Vec<ProbeOperation>>>);

impl<T: Send> SendOperation<T, ProbeOperation> for CountingPolicy {
    type Output = Result<Response<T>, Error>;
    async fn send<E: Endpoint<T> + Send>(
        &self,
        operation: ProbeOperation,
        endpoint: E,
    ) -> Self::Output {
        self.0.lock().unwrap().push(operation);
        concord::SendWith::send(&Policy, endpoint).await
    }
}

concord::api! {
    pub client Probe(policy: CountingPolicy) {
        base "http://127.0.0.1";
        format concord::Json;
        send policy -> Result<Response<_>, Error>;
        operations ProbeOperation;
        GET list(query per_page: u32 = 7) -> ProbePage {
            path "items";
            page next url next;
        }
    }
}

#[tokio::test]
async fn followed_operation_uses_the_same_stateful_operation_aware_policy() {
    let server = TestServer::start(|request: &Recorded| match request.target.as_str() {
        "/items?per_page=7" => Reply::json(
            200,
            format!(
                r#"{{"items":[1],"next":"http://{}/items?b=2&a=1&cursor={CANARY}"}}"#,
                request.host,
            ),
        ),
        target => {
            assert_eq!(target, format!("/items?b=2&a=1&cursor={CANARY}"));
            Reply::json(200, r#"{"items":[2],"next":null}"#)
        }
    })
    .await;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let api = Probe::new(
        http,
        ProbeConfig {
            policy: CountingPolicy(seen.clone()),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let endpoint = api.list();
    let first = endpoint.clone().send().await.unwrap();
    assert_eq!(first.value.items, [1]);
    let next = ProbeList::next_url(&first.value).unwrap();
    let second = endpoint.follow(next.unwrap()).send().await.unwrap();
    assert_eq!(second.value.items, [2]);
    assert!(ProbeList::next_url(&second.value).unwrap().is_none());
    assert_eq!(
        *seen.lock().unwrap(),
        [ProbeOperation::List, ProbeOperation::List]
    );
    assert_eq!(second.meta.operation().unwrap(), ProbeList::operation());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
