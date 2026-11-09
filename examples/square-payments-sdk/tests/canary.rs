//! Secret inputs, one canary each:
//! - Bearer token: `#[sensitive] header(AUTHORIZATION)` (src/lib.rs:50) through `bearer(..)`;
//!   a second client gets a raw, not-flagged-sensitive `HeaderValue`, so the declaration alone
//!   must redact it.
//! - `#[sensitive] payment_id` of get, complete and cancel (src/lib.rs:69,82,91): path segments.
//! - `#[sensitive] query cursor` (src/lib.rs:75), sent as the traversal's `Resume::At` key, and
//!   the response-only next cursor (`PaymentsPage` Debug withholds it, src/models.rs:196),
//!   which the page traversal sends on page 2.
//! - `CreatePayment` idempotency key and source token (Debug withheld, src/models.rs:93) and
//!   the `CompletePayment` version token (Debug withheld, src/models.rs:155): JSON bodies.
//!
//! The server echoes the token, key, source and version token into response headers, the
//! withheld `SquareError` detail/field (src/error.rs:115) and malformed, wrong-type and
//! truncated bodies. Reads (get, list traversal) and writes (create, complete, cancel) run all
//! seven scenarios. Not covered: native reqwest values, `ProviderError::meta.headers()` and
//! `SquareError` fields read explicitly, `body_unredacted`, and the decoded `Payment` model
//! (its id/version token are returned in clear, so success bodies carry no canary). No wire
//! exemptions.

use concord::reqwest::header::HeaderValue;
use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_square_payments::{
    CompletePayment, CreatePayment, Error, Money, Resume, Square, SquareConfig,
    SquarePaymentsCreate, bearer,
};
use std::{num::NonZeroUsize, time::Duration};

const PAYMENT: &str = include_str!("../fixtures/payment.json");
const AUTHORIZED: &str = include_str!("../fixtures/authorized-payment.json");

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

fn reply(case: usize, success: &str, echo: &str) -> Reply {
    let problem = |code: &str| {
        format!(
            r#"{{"errors":[{{"category":"INVALID_REQUEST_ERROR","code":"{code}","detail":"{echo}","field":"{echo}"}}]}}"#
        )
    };
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(400, problem("IDEMPOTENCY_KEY_REUSED")),
        2 => Reply::json(503, problem("SERVICE_UNAVAILABLE")),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
            format!(r#"{{"payment":{{"id":"{echo}"#).into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
    c.assert_debug_absent("error operation", &e.operation());
}

fn error(c: &mut Canaries, e: &Error) {
    c.assert_debug_absent("SDK error", e);
    c.assert_error_absent("SDK error/source chain", e);
    match e {
        Error::Concord(e) => concord_error(c, e),
        Error::Provider(p) => {
            c.assert_debug_absent("ProviderError", p);
            c.assert_debug_absent("provider metadata", &p.meta);
            c.assert_debug_absent("provider document", &p.document);
            match &p.document {
                Ok(document) => {
                    for entry in &document.errors {
                        c.assert_debug_absent("SquareError", entry);
                    }
                }
                Err(e) => concord_error(c, e),
            }
        }
    }
}

async fn receive<T: std::fmt::Debug>(c: &mut Canaries, prepared: concord::Prepared<T>) {
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

fn check<T: std::fmt::Debug>(
    c: &mut Canaries,
    label: &str,
    case: usize,
    result: &Result<T, Error>,
) {
    c.assert_debug_absent(label, result);
    assert_eq!(result.is_ok(), case == 0, "{label} in case {case}");
    if let Err(e) = result {
        error(c, e);
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("square-payments");
    let token = c.issue();
    let raw_token = c.issue();
    let get_id = c.issue();
    let complete_id = c.issue();
    let cancel_id = c.issue();
    let cursor = c.issue();
    let next_cursor = c.issue();
    let key = c.issue();
    let source = c.issue();
    let version = c.issue();
    let usd = Money {
        amount: 1000,
        currency: "USD".into(),
    };
    let create = CreatePayment::new(&key, &source, usd.clone()).unwrap();
    let completion = CompletePayment::expecting(&version);
    c.assert_debug_absent("CreatePayment", &create);
    c.assert_debug_absent("CompletePayment", &completion);
    let authorization = bearer(&token).unwrap();
    c.assert_debug_absent("bearer header", &authorization);
    let echo = format!("{token} {key} {source} {version}");
    for case in 0..7 {
        let (echo_s, cursor_s, next_s) = (echo.clone(), cursor.clone(), next_cursor.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let at = r.query_pairs().into_iter().find(|(k, _)| k == "cursor");
            let success = match (r.path(), at) {
                (p, _) if p.ends_with("/complete") || p.ends_with("/cancel") => PAYMENT.into(),
                ("/v2/payments", Some((_, v))) if v == cursor_s => format!(
                    r#"{{"payments":[{{"id":"p1","amount_money":{{"amount":1,"currency":"USD"}},"status":"COMPLETED"}}],"cursor":"{next_s}"}}"#
                ),
                ("/v2/payments", Some(_)) => r#"{"payments":[],"cursor":""}"#.into(),
                ("/v2/payments", None) if r.method == "POST" => AUTHORIZED.into(),
                _ => PAYMENT.into(),
            };
            reply(case, &success, &echo_s)
        })
        .await;
        let base = if case == 6 { refused() } else { server.url() };
        let config = SquareConfig {
            authorization: authorization.clone(),
        };
        c.assert_debug_absent("config", &config);
        let api = Square::new(http(), config).with_base_url(&base).unwrap();
        let raw_header = HeaderValue::from_str(&format!("Bearer {raw_token}")).unwrap();
        let raw_api = Square::new(
            http(),
            SquareConfig {
                authorization: raw_header,
            },
        )
        .with_base_url(&base)
        .unwrap();
        let payments = api.payments();
        let create_op = payments.create(create.clone());
        let get_op = payments.get(&get_id);
        let complete_op = payments.complete(&complete_id, completion.clone());
        let cancel_op = payments.cancel(&cancel_id);
        let list_op = payments.list().set_cursor(&cursor).set_limit(10u32);
        let raw_get = raw_api.payments().get("p1");
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("raw-header client", &raw_api);
        c.assert_debug_absent("payments scope", &payments);
        c.assert_debug_absent("create endpoint", &create_op);
        c.assert_debug_absent("get endpoint", &get_op);
        c.assert_debug_absent("complete endpoint", &complete_op);
        c.assert_debug_absent("cancel endpoint", &cancel_op);
        c.assert_debug_absent("list endpoint", &list_op);
        c.assert_debug_absent("raw get endpoint", &raw_get);
        c.assert_debug_absent("create Prepared", &create_op.clone().prepare().unwrap());
        c.assert_debug_absent("complete Prepared", &complete_op.clone().prepare().unwrap());
        c.assert_debug_absent("cancel Prepared", &cancel_op.clone().prepare().unwrap());
        c.assert_debug_absent("list Prepared", &list_op.clone().prepare().unwrap());
        receive(&mut c, get_op.clone().prepare().unwrap()).await;
        receive(&mut c, raw_get.clone().prepare().unwrap()).await;

        let created = create_op.send().await;
        check(&mut c, "create Result", case, &created);
        let read = get_op.send().await;
        check(&mut c, "get Result", case, &read);
        let completed = complete_op.send().await;
        check(&mut c, "complete Result", case, &completed);
        let cancelled = cancel_op.send().await;
        check(&mut c, "cancel Result", case, &cancelled);
        let raw_read = raw_get.send().await;
        check(&mut c, "raw get Result", case, &raw_read);
        if let Ok(response) = &created {
            c.assert_debug_absent("create response", response);
            assert_eq!(response.payment.status, "APPROVED");
        }
        if let Err(e) = &created {
            match case {
                1 => {
                    let Error::Provider(p) = e else {
                        panic!("{e:?}")
                    };
                    assert_eq!(e.to_string(), "Square HTTP 400");
                    assert_eq!(p.meta.status().as_u16(), 400);
                    assert_eq!(p.meta.operation(), Some(SquarePaymentsCreate::operation()));
                    assert!(p.is_idempotency_mismatch());
                    // The withheld detail really reached the decoder.
                    assert_eq!(
                        p.document.as_ref().unwrap().errors[0].detail,
                        Some(echo.clone())
                    );
                    assert!(!e.outcome_unknown());
                }
                2 => assert!(e.create_outcome_unknown()),
                3 => assert!(e.to_string().contains("Square.payments.create")),
                _ => {}
            }
        }

        let mut pages = payments
            .list()
            .set_limit(10u32)
            .pages(Resume::At(cursor.clone()), NonZeroUsize::new(3).unwrap());
        c.assert_debug_absent("page traversal", &pages);
        let first = pages.next_page().await;
        c.assert_debug_absent("first page Result", &first);
        match first {
            Ok(page) => {
                let page = page.unwrap();
                c.assert_debug_absent("first page", &page);
                assert_eq!(page.next.as_deref(), Some(next_cursor.as_str()));
                c.assert_debug_absent("traversal checkpoint", &pages.checkpoint());
                let second = pages.next_page().await;
                c.assert_debug_absent("second page Result", &second);
                assert!(second.unwrap().unwrap().next.is_none());
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_error_absent("PagesError/source chain", &e);
                c.assert_debug_absent("PagesError resume", &e.resume());
                if let Some(p) = e.provider_error() {
                    error(&mut c, p);
                }
                assert!(matches!(e.resume(), Some(Resume::At(k)) if *k == cursor));
            }
        }
        c.assert_debug_absent("traversal checkpoint", &pages.checkpoint());
        c.assert_debug_absent("traversal progress", &pages.progress());
        c.assert_debug_absent("traversal report", &pages.finish());

        if case == 0 {
            let requests = server.requests();
            c.assert_on_wire(&requests);
            assert!(
                requests
                    .iter()
                    .any(|r| r.header("authorization")
                        == Some(format!("Bearer {raw_token}").as_str()))
            );
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    // Local refusals that were handed a secret retain none of it.
    let e = bearer(&format!("{token}\n")).unwrap_err();
    c.assert_debug_absent("CredentialError", &e);
    c.assert_error_absent("CredentialError", &e);
    let e = CreatePayment::new(format!("{key}{}", "k".repeat(45)), &source, usd).unwrap_err();
    c.assert_debug_absent("InputError", &e);
    c.assert_error_absent("InputError", &e);
    assert!(c.checked() >= 400);
    println!(
        "square-payments: {} renderings; positive control passed",
        c.checked()
    );
}
