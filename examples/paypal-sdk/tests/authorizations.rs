//! A consumer of authorize-then-capture/void, using only public SDK calls and local fixtures.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_paypal::{
    AuthorizationCaptureRequest, AuthorizationId, AuthorizationStatus, AuthorizeRequest,
    CaptureStatus, Error, Intent, Money, OrderId, OrderRequest, OrderStatus, PayPal,
    PurchaseUnitRequest, RequestId, UncertainCause,
};
use serde_json::{Value, json};
use support::*;

const AUTHORIZATION: &str = "SYNTHAUTH0001X";
const AUTHORIZE_PATH: &str = "/v2/checkout/orders/SYNTH0RDER1A2B3C/authorize";
const AUTH_CAPTURE_PATH: &str = "/v2/payments/authorizations/SYNTHAUTH0001X/capture";
const VOID_PATH: &str = "/v2/payments/authorizations/SYNTHAUTH0001X/void";
const RELEASE_ORDER: &str = "SYNTH0RDER2A2B3C";
const RELEASE_AUTHORIZATION: &str = "SYNTHAUTH0002X";
const RELEASE_ORDER_PATH: &str = "/v2/checkout/orders/SYNTH0RDER2A2B3C";
const RELEASE_AUTHORIZE_PATH: &str = "/v2/checkout/orders/SYNTH0RDER2A2B3C/authorize";
const RELEASE_VOID_PATH: &str = "/v2/payments/authorizations/SYNTHAUTH0002X/void";

fn unit() -> PurchaseUnitRequest {
    PurchaseUnitRequest::new(Money::new("USD", "100.00").unwrap()).unwrap()
}

fn order(status: &str) -> Value {
    json!({"id": ORDER, "status": status, "intent": "AUTHORIZE"})
}

fn authorized_order(status: &str) -> Value {
    let mut result = order("COMPLETED");
    result["purchase_units"] = json!([{"reference_id": "default", "payments": {
        "authorizations": [{"id": AUTHORIZATION, "status": status,
            "amount": {"currency_code": "USD", "value": "100.00"},
            "expiration_time": "2026-12-06T10:00:00Z"}]
    }}]);
    result
}

fn json_reply(status: u16, value: Value) -> Reply {
    Reply::json(status, serde_json::to_vec(&value).unwrap())
}

#[tokio::test]
async fn consumer_creates_authorizes_captures_and_retrieves_then_releases_another_authorization() {
    // Separate orders/authorizations for capture and release; approval is external to the SDK.
    let server = TestServer::start(|r: &Recorded| match (r.method.as_str(), r.path()) {
        ("POST", TOKEN_PATH) => json_fixture(200, "token.json"),
        ("POST", ORDERS_PATH) if r.header("paypal-request-id") == Some("create-true") => {
            let mut result = order("CREATED");
            result["id"] = json!(RELEASE_ORDER);
            json_reply(201, result)
        }
        ("POST", ORDERS_PATH) => json_reply(201, order("CREATED")),
        ("GET", ORDER_PATH) => json_reply(200, order("APPROVED")),
        ("GET", RELEASE_ORDER_PATH) => {
            let mut result = order("APPROVED");
            result["id"] = json!(RELEASE_ORDER);
            json_reply(200, result)
        }
        ("POST", AUTHORIZE_PATH) => {
            assert_eq!(r.body, b"{}");
            json_reply(201, authorized_order("CREATED"))
        }
        ("POST", RELEASE_AUTHORIZE_PATH) => {
            assert_eq!(r.body, b"{}");
            let mut result = authorized_order("CREATED");
            result["id"] = json!(RELEASE_ORDER);
            result["purchase_units"][0]["payments"]["authorizations"][0]["id"] =
                json!(RELEASE_AUTHORIZATION);
            json_reply(201, result)
        }
        ("POST", AUTH_CAPTURE_PATH) => json_fixture(201, "capture.json"),
        ("GET", CAPTURE_PATH) => json_fixture(200, "capture.json"),
        ("POST", RELEASE_VOID_PATH) => Reply::new(204),
        _ => panic!("unexpected request: {} {}", r.method, r.path()),
    })
    .await;
    let paypal = api(&server);

    for release in [false, true] {
        let created = paypal
            .orders()
            .create(OrderRequest::authorize(vec![unit()]).unwrap())
            .set_request_id(RequestId::new(format!("create-{release}")).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(created.intent, Some(Intent::Authorize));
        assert_eq!(created.status, OrderStatus::Created);
        let approved = paypal.order(created.id).get().send().await.unwrap();
        assert_eq!(approved.status, OrderStatus::Approved);
        let authorized = paypal
            .order(approved.id)
            .authorize(AuthorizeRequest::new())
            .set_request_id(RequestId::new(format!("authorize-{release}")).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(authorized.status, OrderStatus::Completed);
        assert_eq!(authorized.captures().count(), 0);
        let authorizations: Vec<_> = authorized.authorizations().collect();
        assert_eq!(authorizations.len(), 1);
        let authorization = authorizations[0];
        assert_eq!(
            authorization.id.as_str(),
            if release {
                RELEASE_AUTHORIZATION
            } else {
                AUTHORIZATION
            }
        );
        assert_eq!(authorization.status, AuthorizationStatus::Created);
        assert_eq!(authorization.amount.as_ref().unwrap().value(), "100.00");
        assert_eq!(
            authorization.expiration_time.as_deref(),
            Some("2026-12-06T10:00:00Z")
        );

        if release {
            paypal
                .authorization(authorization.id.clone())
                .void()
                .set_request_id(RequestId::new("void-true").unwrap())
                .send()
                .await
                .unwrap();
        } else {
            let capture = paypal
                .authorization(authorization.id.clone())
                .capture(
                    AuthorizationCaptureRequest::new()
                        .amount(Money::new("USD", "100.00").unwrap())
                        .unwrap()
                        .final_capture(true),
                )
                .set_request_id(RequestId::new("capture-false").unwrap())
                .send()
                .await
                .unwrap();
            assert_eq!(capture.id.as_str(), CAPTURE);
            assert_eq!(capture.status, CaptureStatus::Completed);
            let shown = paypal
                .capture(capture.id.clone())
                .get()
                .send()
                .await
                .unwrap();
            assert_eq!(shown, capture);
        }
    }
    let requests = server.requests();
    assert_eq!(
        count(&server, "POST", TOKEN_PATH),
        1,
        "shared client-credentials token"
    );
    let token = &requests[0];
    assert_eq!(token.header("authorization"), Some(BASIC));
    assert_eq!(
        token.form_pairs(),
        [("grant_type".into(), "client_credentials".into())]
    );
    assert!(!String::from_utf8_lossy(&token.body).contains("refresh_token"));
    for r in &requests[1..] {
        assert_eq!(
            r.header("authorization"),
            Some(bearer_of("token.json").as_str())
        );
        if r.method == "POST" {
            assert!(r.header("paypal-request-id").is_some());
        }
        if r.path() == ORDERS_PATH {
            assert_eq!(
                serde_json::from_slice::<Value>(&r.body).unwrap(),
                json!({
                "intent": "AUTHORIZE", "purchase_units": [{"amount": {
                    "currency_code": "USD", "value": "100.00"}}]})
            );
        }
        if [
            ORDERS_PATH,
            AUTHORIZE_PATH,
            RELEASE_AUTHORIZE_PATH,
            AUTH_CAPTURE_PATH,
        ]
        .contains(&r.path())
        {
            assert_eq!(r.header("prefer"), Some("return=representation"));
            assert_eq!(r.header("content-type"), Some("application/json"));
        }
        if r.path() == AUTH_CAPTURE_PATH {
            assert_eq!(
                serde_json::from_slice::<Value>(&r.body).unwrap(),
                json!({
                "amount": {"currency_code": "USD", "value": "100.00"}, "final_capture": true})
            );
        }
        if r.path() == RELEASE_VOID_PATH {
            assert!(r.body.is_empty());
        }
    }
    assert_eq!(requests.len(), 10);
    assert_eq!(
        count(&server, "POST", VOID_PATH),
        0,
        "a captured authorization is not voided"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[derive(Clone, Copy, Debug)]
enum Write {
    Authorize,
    Capture,
    Void,
}

impl Write {
    fn path(self) -> &'static str {
        match self {
            Self::Authorize => AUTHORIZE_PATH,
            Self::Capture => AUTH_CAPTURE_PATH,
            Self::Void => VOID_PATH,
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Self::Authorize => "PayPal.order.authorize",
            Self::Capture => "PayPal.authorization.capture",
            Self::Void => "PayPal.authorization.void",
        }
    }

    fn success(self) -> Reply {
        match self {
            Self::Authorize => json_reply(200, authorized_order("CREATED")),
            Self::Capture => json_fixture(200, "capture.json"),
            Self::Void => Reply::new(204),
        }
    }

    async fn send(self, paypal: &PayPal, id: Option<RequestId>) -> Result<(), Error> {
        match self {
            Self::Authorize => {
                let endpoint = paypal
                    .order(OrderId::new(ORDER).unwrap())
                    .authorize(AuthorizeRequest::new());
                let endpoint = if let Some(id) = id {
                    endpoint.set_request_id(id)
                } else {
                    endpoint
                };
                endpoint.send().await.map(|_| ())
            }
            Self::Capture => {
                let endpoint = paypal
                    .authorization(AuthorizationId::new(AUTHORIZATION).unwrap())
                    .capture(
                        AuthorizationCaptureRequest::new()
                            .amount(Money::new("USD", "17.25").unwrap())
                            .unwrap()
                            .final_capture(false),
                    );
                let endpoint = if let Some(id) = id {
                    endpoint.set_request_id(id)
                } else {
                    endpoint
                };
                endpoint.send().await.map(|_| ())
            }
            Self::Void => {
                let endpoint = paypal
                    .authorization(AuthorizationId::new(AUTHORIZATION).unwrap())
                    .void();
                let endpoint = if let Some(id) = id {
                    endpoint.set_request_id(id)
                } else {
                    endpoint
                };
                endpoint.send().await
            }
        }
    }
}

#[tokio::test]
async fn timed_out_writes_repeat_identical_bytes_only_with_the_operations_request_id() {
    for write in [Write::Authorize, Write::Capture, Write::Void] {
        for (with_id, resolved) in [(false, false), (true, true), (true, false)] {
            let attempts = AtomicUsize::new(0);
            let server = serve(move |r| {
                assert_eq!(r.path(), write.path());
                let response = write.success();
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 || !resolved {
                    response.delay(Duration::from_millis(200))
                } else {
                    response
                }
            })
            .await;
            let paypal = builder_with(&server, Duration::from_millis(50))
                .build()
                .unwrap();
            let id = with_id.then(|| RequestId::new("logical-write-1").unwrap());
            let result = write.send(&paypal, id.clone()).await;
            if resolved {
                result.unwrap();
            } else {
                let error = result.unwrap_err();
                let unknown = error.uncertain().unwrap();
                assert_eq!(unknown.operation(), write.operation());
                assert_eq!(unknown.attempts(), if with_id { 2 } else { 1 });
                assert_eq!(unknown.request_id(), id.as_ref());
                assert!(matches!(unknown.cause(), UncertainCause::NoResponse(_)));
            }
            let requests: Vec<_> = server
                .requests()
                .into_iter()
                .filter(|r| r.path() == write.path())
                .collect();
            assert_eq!(requests.len(), if with_id { 2 } else { 1 }, "{write:?}");
            for r in &requests {
                assert_eq!(r.method, "POST");
                assert_eq!(
                    r.header("paypal-request-id"),
                    id.as_ref().map(RequestId::as_str)
                );
                assert_eq!(r.body, requests[0].body);
                assert_eq!(r.target, requests[0].target);
            }
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn provider_refusals_are_not_replayed_even_with_request_ids() {
    for write in [Write::Authorize, Write::Capture, Write::Void] {
        for (status, fixture) in [
            (422, "error-order-not-approved.json"),
            (429, "error-rate-limit-reached.json"),
        ] {
            let server =
                serve(move |_| json_fixture(status, fixture).header("retry-after", "7")).await;
            let error = write
                .send(
                    &api(&server),
                    Some(RequestId::new("refused-write").unwrap()),
                )
                .await
                .unwrap_err();
            let Error::Http(http) = error else {
                panic!("expected first-attempt refusal")
            };
            assert_eq!(http.status().as_u16(), status);
            assert_eq!(http.attempts(), 1);
            assert_eq!(http.operation(), write.operation());
            if status == 429 {
                assert!(http.is_rate_limited());
                assert_eq!(http.retry_after().unwrap(), Some(Duration::from_secs(7)));
            }
            assert_eq!(count(&server, "POST", write.path()), 1);
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn unreadable_authorize_and_authorization_capture_successes_are_uncertain_without_replay() {
    for write in [Write::Authorize, Write::Capture] {
        let server = serve(|_| Reply::json(201, b"{broken")).await;
        let error = write
            .send(&api(&server), Some(RequestId::new("bad-success").unwrap()))
            .await
            .unwrap_err();
        let unknown = error.uncertain().unwrap();
        assert_eq!(unknown.operation(), write.operation());
        assert_eq!(unknown.attempts(), 1);
        assert!(matches!(
            unknown.cause(),
            UncertainCause::UnreadableSuccess(_)
        ));
        assert_eq!(count(&server, "POST", write.path()), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn exhausted_writes_and_later_refusals_remain_uncertain() {
    for write in [Write::Authorize, Write::Capture, Write::Void] {
        for later_refusal in [false, true] {
            let attempts = AtomicUsize::new(0);
            let server = serve(move |_| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 || !later_refusal {
                    json_fixture(500, "error-internal-server-error.json")
                } else {
                    json_fixture(422, "error-order-not-approved.json")
                }
            })
            .await;
            let error = write
                .send(&api(&server), Some(RequestId::new("same-write").unwrap()))
                .await
                .unwrap_err();
            let unknown = error.uncertain().unwrap();
            assert_eq!(unknown.operation(), write.operation());
            assert_eq!(unknown.attempts(), 2);
            assert_eq!(unknown.request_id().unwrap().as_str(), "same-write");
            assert!(match unknown.cause() {
                UncertainCause::ServerError(_) => !later_refusal,
                UncertainCause::LaterRefusal(_) => later_refusal,
                _ => false,
            });
            assert_eq!(count(&server, "POST", write.path()), 2);
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}

#[tokio::test]
async fn void_accepts_a_200_acknowledgement_and_authorize_preserves_unknown_status() {
    let server = serve(|r| {
        if r.path() == VOID_PATH {
            json_reply(200, json!({"id": AUTHORIZATION, "status": "VOIDED"}))
        } else {
            json_reply(201, authorized_order("FUTURE_AUTHORIZATION_STATUS"))
        }
    })
    .await;
    let paypal = api(&server);
    let authorized = paypal
        .order(OrderId::new(ORDER).unwrap())
        .authorize(AuthorizeRequest::new())
        .send()
        .await
        .unwrap();
    assert_eq!(
        authorized.authorizations().next().unwrap().status,
        AuthorizationStatus::Other("FUTURE_AUTHORIZATION_STATUS".into())
    );
    assert_eq!(authorized.captures().count(), 0);
    let shown = paypal
        .order(authorized.id.clone())
        .get_authorized()
        .send()
        .await
        .unwrap();
    assert_eq!(shown, authorized);
    paypal
        .authorization(AuthorizationId::new(AUTHORIZATION).unwrap())
        .void()
        .send()
        .await
        .unwrap();
    assert_eq!(count(&server, "POST", VOID_PATH), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn an_uncertain_authorize_is_reconciled_by_reading_authorizations_without_another_write() {
    let server = serve(|r| match (r.method.as_str(), r.path()) {
        ("POST", AUTHORIZE_PATH) => {
            json_reply(201, authorized_order("CREATED")).delay(Duration::from_millis(200))
        }
        ("GET", ORDER_PATH) => json_reply(200, authorized_order("CREATED")),
        _ => panic!("unexpected request"),
    })
    .await;
    let paypal = builder_with(&server, Duration::from_millis(50))
        .build()
        .unwrap();
    let error = paypal
        .order(OrderId::new(ORDER).unwrap())
        .authorize(AuthorizeRequest::new())
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.uncertain().unwrap().attempts(), 1);
    let reconciled = paypal
        .order(OrderId::new(ORDER).unwrap())
        .get_authorized()
        .send()
        .await
        .unwrap();
    assert_eq!(reconciled.status, OrderStatus::Completed);
    let authorization = reconciled.authorizations().next().unwrap();
    assert_eq!(authorization.id.as_str(), AUTHORIZATION);
    assert_eq!(authorization.status, AuthorizationStatus::Created);
    assert_eq!(count(&server, "POST", AUTHORIZE_PATH), 1);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[2].method, "GET");
    assert_eq!(requests[2].path(), ORDER_PATH);
    assert_eq!(requests[2].header("paypal-request-id"), None);
    assert!(requests[2].body.is_empty());
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn authorization_inputs_and_paths_obey_their_own_contract() {
    // Preserve source compatibility for callers constructing the existing response structs.
    let order = concord_sdk_paypal::Order {
        id: OrderId::new(ORDER).unwrap(),
        status: OrderStatus::Created,
        intent: Some(Intent::Capture),
        purchase_units: vec![concord_sdk_paypal::PurchaseUnit {
            reference_id: None,
            amount: None,
            payments: Some(concord_sdk_paypal::Payments { captures: vec![] }),
        }],
        links: vec![],
        create_time: None,
        update_time: None,
    };
    assert_eq!(order.captures().count(), 0);
    assert!(OrderRequest::authorize(vec![]).is_err());
    assert!(OrderRequest::authorize(vec![unit(), unit()]).is_err());
    assert!(AuthorizationId::new("").is_err());
    assert!(AuthorizationId::new("a b").is_err());
    assert!(AuthorizationId::new("x".repeat(65)).is_err());
    assert!(
        AuthorizationCaptureRequest::new()
            .amount(Money::new("USD", "0").unwrap())
            .is_err()
    );
    assert!(
        AuthorizationCaptureRequest::new()
            .amount(Money::new("USD", "-1").unwrap())
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(AuthorizeRequest::new()).unwrap(),
        json!({})
    );
    assert_eq!(
        serde_json::to_value(AuthorizationCaptureRequest::new()).unwrap(),
        json!({})
    );
    let paypal = PayPal::sandbox(http(Duration::from_secs(1)), credentials());
    let prepared = paypal
        .authorization(AuthorizationId::new("auth/with?separator").unwrap())
        .void()
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api-m.sandbox.paypal.com/v2/payments/authorizations/auth%2Fwith%3Fseparator/void"
    );
}
