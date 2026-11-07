//! Consumer test for delayed capture (maintenance ticket): an order application that
//! authorizes at checkout, captures on shipment with the version token it read, and voids
//! on cancellation. A stale token is acted on; an uncertain complete or cancel is resolved
//! by re-reading the payment, never by an SDK resend. Live provider calls: NOT_RUN.

use concord_example_testserver::{Gate, Recorded, Reply, TestServer};
use concord_sdk_square_payments::concord::Delivery;
use concord_sdk_square_payments::{
    CompletePayment, CreatePayment, Error, Money, Payment, Square, SquareConfig, bearer, transport,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TOKEN: &str = "EAAAl-sandbox-token-delayed";
const SOURCE: &str = "cnon:card-nonce-ok";

type Result<T = (), E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

// ------------------------------------------------------------- Square stand-in

/// Delayed capture as the documentation describes it: `autocomplete: false` holds the payment
/// `APPROVED`; complete moves it to `COMPLETED`, cancel to `CANCELED`; every change issues a
/// new `version_token`, and a complete carrying another token is `VERSION_MISMATCH`.
/// Statuses (400) and the `DELAYED_TRANSACTION_*` answers to a finished payment are fixture
/// choices. `hold` makes the next matching write commit and then never answer.
#[derive(Default)]
struct Sandbox {
    by_key: HashMap<String, (Value, String)>,
    payments: HashMap<String, Value>,
    versions: HashMap<String, u32>,
    hold: Option<&'static str>,
}

impl Sandbox {
    fn bump(&mut self, id: &str, status: Option<&str>) {
        let version = self.versions.entry(id.to_owned()).or_default();
        *version += 1;
        let payment = self.payments.get_mut(id).unwrap();
        payment["version_token"] = json!(format!("vt-{id}-{version}"));
        if let Some(status) = status {
            payment["status"] = json!(status);
        }
    }

    /// Another actor (a dashboard tip adjustment, say) changed the payment.
    fn changed_elsewhere(&mut self, id: &str) {
        self.bump(id, None);
    }

    fn status(&self, id: &str) -> String {
        self.payments[id]["status"].as_str().unwrap().to_owned()
    }
}

fn square_error(status: u16, code: &str, field: Option<&str>) -> Reply {
    let mut entry = json!({
        "category": "INVALID_REQUEST_ERROR", "code": code, "detail": "provider detail"
    });
    if let Some(field) = field {
        entry["field"] = json!(field);
    }
    Reply::json(status, json!({ "errors": [entry] }).to_string())
}

fn answer(payment: &Value) -> Reply {
    Reply::json(200, json!({ "payment": payment }).to_string())
}

fn sandbox_reply(state: &Mutex<Sandbox>, request: &Recorded) -> Reply {
    let mut state = state.lock().unwrap();
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(request.header("square-version"), Some("2026-09-16"));
    let path = request.path().to_owned();
    let rest = path.strip_prefix("/v2/payments").unwrap();
    match (request.method.as_str(), rest.rsplit_once('/')) {
        ("POST", None) if rest.is_empty() => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let key = body["idempotency_key"].as_str().unwrap().to_owned();
            if let Some((stored, id)) = state.by_key.get(&key) {
                if *stored == body {
                    return answer(&state.payments[id]);
                }
                return square_error(400, "IDEMPOTENCY_KEY_REUSED", Some("idempotency_key"));
            }
            let id = format!("pay-{}", state.payments.len() + 1);
            let status = if body["autocomplete"] == json!(false) {
                "APPROVED"
            } else {
                "COMPLETED"
            };
            state.payments.insert(
                id.clone(),
                json!({
                    "id": id, "amount_money": body["amount_money"], "status": status,
                    "delay_action": "CANCEL", "delay_duration": "P7D", "source_type": "CARD",
                }),
            );
            state.bump(&id, None);
            state.by_key.insert(key, (body, id.clone()));
            answer(&state.payments[&id])
        }
        ("GET", Some(("", id))) => match state.payments.get(id) {
            Some(payment) => answer(payment),
            None => square_error(404, "NOT_FOUND", None),
        },
        ("POST", Some((prefix, action @ ("complete" | "cancel")))) => {
            let id = prefix.strip_prefix('/').unwrap().to_owned();
            if !state.payments.contains_key(&id) {
                return square_error(404, "NOT_FOUND", None);
            }
            match state.status(&id).as_str() {
                "APPROVED" => {}
                "COMPLETED" => return square_error(400, "DELAYED_TRANSACTION_CAPTURED", None),
                "CANCELED" => return square_error(400, "DELAYED_TRANSACTION_CANCELED", None),
                other => panic!("unexpected status {other}"),
            }
            if action == "complete" {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                if let Some(expected) = body.get("version_token")
                    && *expected != state.payments[&id]["version_token"]
                {
                    return square_error(400, "VERSION_MISMATCH", Some("version_token"));
                }
                state.bump(&id, Some("COMPLETED"));
            } else {
                assert!(request.body.is_empty(), "cancel has no body");
                state.bump(&id, Some("CANCELED"));
            }
            let reply = answer(&state.payments[&id]);
            if state.hold == Some(action) {
                state.hold = None;
                reply.after(Gate::new()) // committed, never answered
            } else {
                reply
            }
        }
        _ => Reply::new(405),
    }
}

// ------------------------------------------------------------- the application

fn usd(amount: i64) -> Money {
    Money {
        amount,
        currency: "USD".into(),
    }
}

/// Checkout: hold the funds. The key is the order's, persisted by the application.
async fn authorize(api: &Square, order: &str, amount: i64) -> Result<Payment, Error> {
    let body = CreatePayment::authorization(format!("order-{order}"), SOURCE, usd(amount)).unwrap();
    Ok(api.payments().create(body).send().await?.payment)
}

/// What the shipment routine learned.
#[derive(Debug)]
enum Capture {
    /// Square captured the version the application held.
    Completed(Payment),
    /// The payment changed since it was read; nothing was captured. The fresh read is here
    /// for the application's decision.
    Changed(Payment),
    /// The complete may or may not have happened; the re-read says what Square has.
    Uncertain { first: Error, current: Payment },
}

/// Shipment: one complete with the held version token. Never resent by the SDK or here.
async fn capture(api: &Square, held: &Payment) -> Result<Capture, Error> {
    let completion = match &held.version_token {
        Some(token) => CompletePayment::expecting(token.clone()),
        None => CompletePayment::unconditional(),
    };
    match api.payments().complete(&held.id, completion).send().await {
        Ok(response) => Ok(Capture::Completed(response.payment)),
        Err(Error::Provider(provider)) if provider.is_version_mismatch() => Ok(Capture::Changed(
            api.payments().get(&held.id).send().await?.payment,
        )),
        Err(first) if first.outcome_unknown() => {
            let current = api.payments().get(&held.id).send().await?.payment;
            Ok(Capture::Uncertain { first, current })
        }
        Err(error) => Err(error),
    }
}

/// Order cancellation: one void; an uncertain one is resolved by a re-read.
async fn void(api: &Square, held: &Payment) -> Result<Payment, Error> {
    match api.payments().cancel(&held.id).send().await {
        Ok(response) => Ok(response.payment),
        Err(error) if error.outcome_unknown() => {
            assert!(!error.create_outcome_unknown());
            Ok(api.payments().get(&held.id).send().await?.payment)
        }
        Err(error) => Err(error),
    }
}

fn completes(server: &TestServer) -> usize {
    server
        .requests()
        .iter()
        .filter(|request| request.path().ends_with("/complete"))
        .count()
}

#[tokio::test]
async fn authorize_capture_void_with_stale_and_uncertain_outcomes() -> Result {
    let state = Arc::new(Mutex::new(Sandbox::default()));
    let server = {
        let state = state.clone();
        TestServer::start(move |request: &Recorded| sandbox_reply(&state, request)).await
    };
    let api = Square::new(
        transport(Duration::from_millis(300))?,
        SquareConfig {
            authorization: bearer(TOKEN)?,
        },
    )
    .with_base_url(server.url())?;

    // 1. Authorize, inspect, capture with the token read at checkout.
    let held = authorize(&api, "1001", 4200).await?;
    assert_eq!(held.status, "APPROVED");
    let token = held.version_token.clone().expect("Square returns a token");
    let sent: Value = serde_json::from_slice(&server.requests()[0].body)?;
    assert_eq!(sent["autocomplete"], json!(false));
    assert_eq!(api.payments().get(&held.id).send().await?.payment, held);
    let Capture::Completed(captured) = capture(&api, &held).await? else {
        panic!("a fresh token captures")
    };
    assert_eq!(captured.status, "COMPLETED");
    assert_eq!(captured.amount_money, usd(4200));
    assert_ne!(captured.version_token.as_deref(), Some(token.as_str()));
    let complete = server.requests().pop().unwrap();
    assert_eq!(
        complete.target,
        format!("/v2/payments/{}/complete", held.id)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&complete.body)?,
        json!({ "version_token": token })
    );

    // 2. Stale token: the payment changed after checkout. The mismatch is typed, nothing was
    //    captured, and the application decides on the fresh read (here: capture that version).
    let held = authorize(&api, "1002", 1500).await?;
    state.lock().unwrap().changed_elsewhere(&held.id);
    let before = server.requests().len();
    let Capture::Changed(fresh) = capture(&api, &held).await? else {
        panic!("a stale token is refused")
    };
    assert_eq!(fresh.status, "APPROVED", "nothing was captured");
    assert_ne!(fresh.version_token, held.version_token);
    assert_eq!(
        server.requests().len(),
        before + 2,
        "one complete, one re-read"
    );
    let Capture::Completed(captured) = capture(&api, &fresh).await? else {
        panic!("the fresh version captures")
    };
    assert_eq!(captured.status, "COMPLETED");
    assert_eq!(state.lock().unwrap().status(&held.id), "COMPLETED");

    // 3. Uncertain complete: Square captured, then the answer never came. The routine re-reads
    //    and learns it is done; exactly one complete was sent for this order.
    let held = authorize(&api, "1003", 900).await?;
    state.lock().unwrap().hold = Some("complete");
    let before = completes(&server);
    let Capture::Uncertain { first, current } = capture(&api, &held).await? else {
        panic!("the complete timed out")
    };
    assert!(first.outcome_unknown());
    assert!(!first.create_outcome_unknown(), "not a create replay");
    let Error::Concord(timeout) = &first else {
        panic!("a timeout is a Concord error")
    };
    assert_eq!(timeout.delivery(), Delivery::Unknown);
    assert_eq!(
        timeout.operation().map(|op| op.name()),
        Some("Square.payments.complete")
    );
    assert!(!format!("{first:?} {first}").contains(&held.id));
    assert_eq!(current.status, "COMPLETED");
    assert_eq!(completes(&server), before + 1, "never resent");

    // 4. Void an authorization, including an uncertain void resolved by a re-read; a later
    //    capture of the voided payment is an ordinary typed refusal, not a mismatch.
    let held = authorize(&api, "1004", 700).await?;
    let voided = void(&api, &held).await?;
    assert_eq!(voided.status, "CANCELED");
    let cancel = server.requests().pop().unwrap();
    assert_eq!(cancel.target, format!("/v2/payments/{}/cancel", held.id));
    assert!(cancel.body.is_empty());
    let held = authorize(&api, "1005", 800).await?;
    state.lock().unwrap().hold = Some("cancel");
    let before = server.requests().len();
    assert_eq!(void(&api, &held).await?.status, "CANCELED");
    assert_eq!(
        server.requests().len(),
        before + 2,
        "one cancel, one re-read"
    );
    let error = match capture(&api, &held).await {
        Err(error) => error,
        Ok(other) => panic!("a voided payment cannot be captured: {other:?}"),
    };
    assert!(!error.outcome_unknown(), "a 400 is an answer");
    let Error::Provider(refused) = &error else {
        panic!("a provider refusal")
    };
    assert!(!refused.is_version_mismatch());
    assert_eq!(
        refused.document.as_ref().unwrap().errors[0].code,
        "DELAYED_TRANSACTION_CANCELED"
    );

    // 5. An application that kept only the payment ID completes unconditionally (`{}`).
    let held = authorize(&api, "1006", 300).await?;
    let id_only = Payment {
        version_token: None,
        ..held
    };
    let Capture::Completed(captured) = capture(&api, &id_only).await? else {
        panic!("an unconditional complete captures")
    };
    assert_eq!(captured.status, "COMPLETED");
    assert_eq!(server.requests().pop().unwrap().body, b"{}");

    // 6. Immediate capture is unchanged for existing callers.
    let immediate = CreatePayment::new("order-1007", SOURCE, usd(100))?;
    let paid = api.payments().create(immediate).send().await?.payment;
    assert_eq!(paid.status, "COMPLETED");
    let sent: Value = serde_json::from_slice(&server.requests().pop().unwrap().body)?;
    assert_eq!(sent["autocomplete"], json!(true));
    assert_eq!(server.shutdown().await.open_after, 0);
    Ok(())
}
