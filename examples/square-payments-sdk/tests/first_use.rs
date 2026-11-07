//! Fresh-consumer application tests (see `FIRST_USE.md`).
//!
//! Two caller tasks, one per workflow, against a loopback stand-in for the Square
//! sandbox, plus one test for the failure shapes predicted in `FIRST_USE.md`.
//! Live provider calls: NOT_RUN.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_square_payments::concord::{Delivery, ErrorKind};
use concord_sdk_square_payments::{
    CreatePayment, CredentialError, Error, InputError, Money, PagesError, Payment, Resume, Square,
    SquareConfig, bearer, transport,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TOKEN: &str = "EAAAl-sandbox-token-7f3c";
const VERSION: &str = "2026-09-16";
const SOURCE: &str = "cnon:card-nonce-ok";

type Result<T = (), E = Box<dyn std::error::Error>> = std::result::Result<T, E>;

// ---------------------------------------------------------------- shared fixtures

fn payment_json(id: &str, amount: i64, currency: &str, status: &str) -> Value {
    // Realistic envelope: fields the SDK does not model must be ignored.
    json!({
        "id": id,
        "created_at": "2026-11-07T04:40:00.000Z",
        "amount_money": { "amount": amount, "currency": currency },
        "total_money": { "amount": amount, "currency": currency },
        "status": status,
        "source_type": "CARD",
        "card_details": { "status": "CAPTURED", "card": { "card_brand": "VISA", "last_4": "1111" } },
        "location_id": "L88917AVBK2S5",
        "receipt_number": "R2B3",
        "receipt_url": "https://squareupsandbox.com/receipt/preview/R2B3",
        "version_token": "TPtNEOBOa6Qq6E3C3IjckSVOM6b3hMbfhjvTxHBQUsB6o"
    })
}

fn square_error(
    status: u16,
    category: &str,
    code: &str,
    detail: &str,
    field: Option<&str>,
) -> Reply {
    let mut entry = json!({ "category": category, "code": code, "detail": detail });
    if let Some(field) = field {
        entry["field"] = json!(field);
    }
    Reply::json(status, json!({ "errors": [entry] }).to_string())
}

fn client(base: &str, timeout: Duration) -> Result<Square> {
    let api = Square::new(
        transport(timeout)?,
        SquareConfig {
            authorization: bearer(TOKEN)?,
        },
    )
    .with_base_url(base)?;
    Ok(api)
}

/// Headers every call must carry; no idempotency header is ever sent.
fn assert_common_headers(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer {TOKEN}").as_str())
    );
    assert_eq!(request.header("square-version"), Some(VERSION));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert!(
        request
            .headers
            .iter()
            .all(|(name, _)| !name.contains("idempotency"))
    );
}

fn assert_redacted(text: &str, secrets: &[&str]) {
    for secret in secrets {
        assert!(
            !text.contains(secret),
            "diagnostic leaked {secret:?}: {text}"
        );
    }
}

// ---------------------------------------------------------------- workflow 1

/// A Square stand-in that honours idempotency the way the docs describe it: the first
/// request with a key commits the payment; the same key with the same body returns the
/// stored payment; the same key with a different body is `IDEMPOTENCY_KEY_REUSED`.
/// The first committed create is answered late, so the caller times out *after* the commit.
struct SquareSandbox {
    by_key: HashMap<String, (Value, String)>,
    payments: HashMap<String, Value>,
    next_id: usize,
}

fn sandbox_reply(
    state: &Mutex<SquareSandbox>,
    delay_first: &AtomicBool,
    request: &Recorded,
) -> Reply {
    let mut state = state.lock().unwrap();
    match (request.method.as_str(), request.path()) {
        ("POST", "/v2/payments") => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let key = body["idempotency_key"].as_str().unwrap().to_owned();
            if let Some((stored_body, id)) = state.by_key.get(&key) {
                if *stored_body == body {
                    let payment = state.payments[id].clone();
                    return Reply::json(200, json!({ "payment": payment }).to_string());
                }
                return square_error(
                    400,
                    "INVALID_REQUEST_ERROR",
                    "IDEMPOTENCY_KEY_REUSED",
                    "The idempotency key 7b0f3ec5 was reused with different parameters",
                    Some("idempotency_key"),
                );
            }
            state.next_id += 1;
            let id = format!("R2B3Z8WMVt3EAmzYWLZvz7Y69Eb{:02}", state.next_id);
            let money = &body["amount_money"];
            let payment = payment_json(
                &id,
                money["amount"].as_i64().unwrap(),
                money["currency"].as_str().unwrap(),
                "COMPLETED",
            );
            state.payments.insert(id.clone(), payment.clone());
            state.by_key.insert(key, (body, id));
            let reply = Reply::json(200, json!({ "payment": payment }).to_string());
            if delay_first.swap(false, Ordering::SeqCst) {
                reply.delay(Duration::from_secs(3)) // committed, but answered too late
            } else {
                reply
            }
        }
        ("GET", path) => match path
            .strip_prefix("/v2/payments/")
            .and_then(|id| state.payments.get(id))
        {
            Some(payment) => Reply::json(200, json!({ "payment": payment }).to_string()),
            None => square_error(
                404,
                "INVALID_REQUEST_ERROR",
                "NOT_FOUND",
                "Could not find payment",
                None,
            ),
        },
        _ => Reply::new(405),
    }
}

/// What the application's charge routine reports.
struct Charge {
    payment: Payment,
    /// The failure of the first send when the routine had to replay it.
    replayed_after: Option<Error>,
}

/// The application's charge routine: one send; after a timeout, exactly one explicit replay
/// of the retained, unchanged operation (same caller key, same body). Never a new key.
async fn charge(api: &Square, body: CreatePayment) -> Result<Charge, Error> {
    let operation = api.payments().create(body);
    let retained = operation.clone();
    match operation.send().await {
        Ok(response) => Ok(Charge {
            payment: response.payment,
            replayed_after: None,
        }),
        Err(Error::Concord(error)) if error.transport().is_some_and(|e| e.is_timeout()) => {
            let first = Error::Concord(error);
            let payment = retained.send().await?.payment;
            Ok(Charge {
                payment,
                replayed_after: Some(first),
            })
        }
        Err(error) => Err(error),
    }
}

#[tokio::test]
async fn workflow_1_create_with_caller_key_replay_after_timeout_then_retrieve() -> Result {
    let state = Arc::new(Mutex::new(SquareSandbox {
        by_key: HashMap::new(),
        payments: HashMap::new(),
        next_id: 0,
    }));
    let delay_first = Arc::new(AtomicBool::new(true));
    let server = {
        let (state, delay_first) = (state.clone(), delay_first.clone());
        TestServer::start(move |request: &Recorded| sandbox_reply(&state, &delay_first, request))
            .await
    };
    let api = client(&server.url(), Duration::from_millis(400))?;
    assert_redacted(&format!("{api:?}"), &[TOKEN]);

    // Local validation happens before any request exists.
    let key = "7b0f3ec5-086a-4871-8f13-3c81b3875218"; // caller-chosen, persisted by the app
    assert_eq!(
        CreatePayment::new("", SOURCE, usd(1000)).err(),
        Some(InputError::IdempotencyKey)
    );
    assert_eq!(
        CreatePayment::new("é".repeat(23), SOURCE, usd(1000)).err(),
        Some(InputError::IdempotencyKey)
    );
    assert!(CreatePayment::new("k".repeat(45), SOURCE, usd(1000)).is_ok());
    assert_eq!(
        CreatePayment::new(key, "", usd(1000)).err(),
        Some(InputError::Source)
    );
    assert_eq!(
        CreatePayment::new(key, SOURCE, usd(0)).err(),
        Some(InputError::Amount)
    );
    let lower = Money {
        amount: 1000,
        currency: "usd".into(),
    };
    assert_eq!(
        CreatePayment::new(key, SOURCE, lower).err(),
        Some(InputError::Currency)
    );
    assert!(server.requests().is_empty());

    // 1. Create: the first send times out after Square committed; the routine replays once.
    let body = CreatePayment::new(key, SOURCE, usd(1000))?;
    assert_redacted(&format!("{body:?}"), &[key, SOURCE]);
    let charged = charge(&api, body).await?;
    let first = charged.replayed_after.expect("the first send timed out");
    let Error::Concord(concord) = &first else {
        panic!("timeout is a Concord error")
    };
    assert_eq!(concord.kind(), ErrorKind::Transport);
    assert_eq!(concord.delivery(), Delivery::Unknown);
    assert_eq!(
        concord.operation().map(|op| op.name()),
        Some("Square.payments.create")
    );
    assert!(first.create_outcome_unknown());
    assert_redacted(&format!("{first:?} {first}"), &[TOKEN, key, SOURCE]);

    let posts = server.requests();
    assert_eq!(
        posts.len(),
        2,
        "one send plus one explicit replay; no SDK retry"
    );
    for post in &posts {
        assert_eq!(
            (post.method.as_str(), post.target.as_str()),
            ("POST", "/v2/payments")
        );
        assert_common_headers(post);
        assert_eq!(post.header("content-type"), Some("application/json"));
    }
    assert_eq!(posts[0].body, posts[1].body, "the replay is byte-identical");
    let sent: Value = serde_json::from_slice(&posts[0].body)?;
    assert_eq!(
        sent,
        json!({
            "idempotency_key": key,
            "source_id": SOURCE,
            "amount_money": { "amount": 1000, "currency": "USD" },
            "autocomplete": true
        })
    );
    let created = charged.payment;
    assert_eq!(
        state.lock().unwrap().payments.len(),
        1,
        "one charge despite two POSTs"
    );
    assert_eq!(created.amount_money, usd(1000));
    assert_eq!(created.status, "COMPLETED");

    // 2. Retrieve the payment that the replay reported.
    let inspected = api.payments().get(&created.id).send().await?.payment;
    assert_eq!(inspected, created);
    let get = server.requests().pop().unwrap();
    assert_eq!(get.method, "GET");
    assert_eq!(get.target, format!("/v2/payments/{}", created.id));
    assert_common_headers(&get);
    assert_eq!(get.header("content-type"), None);
    assert!(get.body.is_empty());

    // 3. A mismatched replay — a restarted process rebuilt the body with another amount
    //    under the persisted key — is an error the routine surfaces, not a new charge.
    let rebuilt = CreatePayment::new(key, SOURCE, usd(1500))?;
    let mismatch = match charge(&api, rebuilt).await {
        Err(error) => error,
        Ok(_) => panic!("a changed body under a used key must not succeed"),
    };
    assert!(!mismatch.create_outcome_unknown(), "a 400 is an answer");
    let Error::Provider(provider) = &mismatch else {
        panic!("400 is a provider error")
    };
    assert!(provider.is_idempotency_mismatch());
    assert_eq!(provider.meta.status().as_u16(), 400);
    assert_eq!(
        provider.meta.operation().map(|op| op.name()),
        Some("Square.payments.create")
    );
    let document = provider.document.as_ref().expect("typed Square errors");
    assert_eq!(document.errors.len(), 1);
    assert_eq!(document.errors[0].category, "INVALID_REQUEST_ERROR");
    assert_eq!(document.errors[0].field.as_deref(), Some("idempotency_key"));
    assert_redacted(
        &format!("{mismatch:?} {mismatch}"),
        &[TOKEN, key, "7b0f3ec5 was reused"],
    );
    assert_eq!(
        server.requests().len(),
        4,
        "the mismatch was sent once, never retried"
    );
    assert_eq!(state.lock().unwrap().payments.len(), 1);

    // An ID is exactly one path segment.
    let missing = api
        .payments()
        .get("R2B3/../refunds")
        .send()
        .await
        .unwrap_err();
    assert_eq!(
        server.requests().pop().unwrap().target,
        "/v2/payments/R2B3%2F..%2Frefunds"
    );
    let Error::Provider(not_found) = &missing else {
        panic!("404 is a provider error")
    };
    assert_eq!(not_found.meta.status().as_u16(), 404);
    assert_eq!(
        not_found.document.as_ref().unwrap().errors[0].code,
        "NOT_FOUND"
    );
    server.shutdown().await;
    Ok(())
}

fn usd(amount: i64) -> Money {
    Money {
        amount,
        currency: "USD".into(),
    }
}

// ---------------------------------------------------------------- workflow 2

const C1: &str = "bGFzdD1HUTNa+page/2==";
const C2: &str = "bGFzdD1ac3Zx+page/3==";

/// Account listing: page 1 (two payments, cursor C1), page 2 (empty, cursor C2), page 3
/// (one payment, last). `fail_c1` makes the next `fail_c1` requests for C1 answer 503.
fn listing_reply(request: &Recorded, fail_c1: &AtomicUsize, empty_final_cursor: bool) -> Reply {
    let page = |payments: Vec<Value>, cursor: Option<&str>| {
        let mut body = json!({});
        if !payments.is_empty() {
            body["payments"] = json!(payments); // Square omits an empty array
        }
        if let Some(cursor) = cursor {
            body["cursor"] = json!(cursor);
        }
        Reply::json(200, body.to_string())
    };
    match request.path() {
        "/v2/payments" => match request.query_all("cursor").first().map(String::as_str) {
            None => page(
                vec![
                    payment_json("GQTFp1ZlXdpoW4o6eGiZhbjosiDFf", 2500, "USD", "COMPLETED"),
                    payment_json("ZsvqtVkKbRLQkb9oNqsh7lrPtNYbE", 799, "USD", "APPROVED"),
                ],
                Some(C1),
            ),
            Some(C1) => {
                if fail_c1.load(Ordering::SeqCst) > 0 {
                    fail_c1.fetch_sub(1, Ordering::SeqCst);
                    return square_error(
                        503,
                        "API_ERROR",
                        "SERVICE_UNAVAILABLE",
                        "try later",
                        None,
                    )
                    .header("retry-after", "2");
                }
                page(vec![], Some(C2))
            }
            Some(C2) => page(
                vec![payment_json(
                    "bP9mAsEMYPUGjjGNaNO5ZDVyLhSZY",
                    12000,
                    "USD",
                    "FAILED",
                )],
                if empty_final_cursor { Some("") } else { None },
            ),
            Some(_) => square_error(
                400,
                "INVALID_REQUEST_ERROR",
                "INVALID_CURSOR",
                "bad cursor",
                Some("cursor"),
            ),
        },
        path => {
            let id = path.strip_prefix("/v2/payments/").unwrap();
            Reply::json(
                200,
                json!({ "payment": payment_json(id, 2500, "USD", "COMPLETED") }).to_string(),
            )
        }
    }
}

fn ids(payments: &[Payment]) -> Vec<&str> {
    payments.iter().map(|payment| payment.id.as_str()).collect()
}

#[tokio::test]
async fn workflow_2_page_account_payments_by_cursor_and_inspect_one() -> Result {
    let fail_c1 = Arc::new(AtomicUsize::new(0));
    let server = {
        let fail_c1 = fail_c1.clone();
        TestServer::start(move |request: &Recorded| listing_reply(request, &fail_c1, false)).await
    };
    let api = client(&server.url(), Duration::from_secs(5))?;
    let ten = NonZeroUsize::new(10).unwrap();

    // 1. Full traversal; building it sends nothing.
    let mut pages = api
        .payments()
        .list()
        .set_limit(2u32)
        .pages(Resume::Start, ten);
    assert!(server.requests().is_empty());
    let mut seen: Vec<Payment> = Vec::new();
    let mut page_sizes = Vec::new();
    while let Some(page) = pages.next_page().await? {
        page_sizes.push(page.items.len());
        seen.extend(page.items);
    }
    assert_eq!(
        page_sizes,
        [2, 0, 1],
        "the empty page with a cursor did not end the listing"
    );
    assert_eq!(
        ids(&seen),
        [
            "GQTFp1ZlXdpoW4o6eGiZhbjosiDFf",
            "ZsvqtVkKbRLQkb9oNqsh7lrPtNYbE",
            "bP9mAsEMYPUGjjGNaNO5ZDVyLhSZY"
        ]
    );
    assert_eq!(seen[1].status, "APPROVED");
    assert_eq!(
        seen[2].amount_money,
        Money {
            amount: 12000,
            currency: "USD".into()
        }
    );
    assert!(pages.next_page().await?.is_none(), "completion is sticky");
    assert!(pages.checkpoint().is_none());

    let lists = server.requests();
    assert_eq!(lists.len(), 3, "no request after the last page");
    let queries: Vec<_> = lists.iter().map(Recorded::query_pairs).collect();
    let pair = |k: &str, v: &str| (k.to_owned(), v.to_owned());
    assert_eq!(queries[0], [pair("limit", "2")]);
    assert_eq!(queries[1], [pair("cursor", C1), pair("limit", "2")]);
    assert_eq!(queries[2], [pair("cursor", C2), pair("limit", "2")]);
    for list in &lists {
        assert_eq!((list.method.as_str(), list.path()), ("GET", "/v2/payments"));
        assert_common_headers(list);
        assert_eq!(list.header("content-type"), None);
    }
    assert!(
        !lists[1].target.contains('+'),
        "an opaque cursor is percent-encoded: {}",
        lists[1].target
    );

    // 2. Inspect one returned payment.
    let one = api.payments().get(&seen[0].id).send().await?.payment;
    assert_eq!(one.id, seen[0].id);
    assert_eq!(one.amount_money.amount, 2500);
    let get = server.requests().pop().unwrap();
    assert_eq!(get.target, "/v2/payments/GQTFp1ZlXdpoW4o6eGiZhbjosiDFf");
    assert_common_headers(&get);

    // 3. One page by cursor, without a traversal.
    let page = api
        .payments()
        .list()
        .set_limit(2u32)
        .maybe_cursor(Some(C2.to_owned()))
        .send()
        .await?;
    assert_eq!(ids(&page.payments), ["bP9mAsEMYPUGjjGNaNO5ZDVyLhSZY"]);
    assert_eq!(page.cursor, None);
    let empty = api.payments().list().set_cursor(C1).send().await?;
    assert!(empty.payments.is_empty());
    assert_eq!(empty.cursor.as_deref(), Some(C2));

    // 4. A page bound with a remaining cursor is not completion.
    let before = server.requests().len();
    let mut bounded = api
        .payments()
        .list()
        .set_limit(2u32)
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap());
    assert_eq!(
        bounded.next_page().await?.map(|page| page.items.len()),
        Some(2)
    );
    match bounded.next_page().await {
        Err(PagesError::Bounded { resume, .. }) => assert_eq!(resume, Resume::At(C1.to_owned())),
        other => panic!("expected Bounded, got {other:?}"),
    }
    assert!(matches!(
        bounded.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), before + 1);

    // 5. Partial result: page 2 fails once (503); the app keeps page 1 and resumes at the
    //    failed cursor with the same limit, without re-reading page 1.
    fail_c1.store(1, Ordering::SeqCst);
    let before = server.requests().len();
    let mut pages = api
        .payments()
        .list()
        .set_limit(2u32)
        .pages(Resume::Start, ten);
    let mut kept: Vec<Payment> = Vec::new();
    let failure = loop {
        match pages.next_page().await {
            Ok(Some(page)) => kept.extend(page.items),
            Ok(None) => panic!("the listing cannot complete while page 2 fails"),
            Err(error) => break error,
        }
    };
    assert_eq!(kept.len(), 2);
    assert_redacted(
        &format!("{failure:?} {failure} {pages:?}"),
        &[C1, TOKEN, "try later"],
    );
    let resume = match failure {
        PagesError::Provider {
            error: Error::Provider(provider),
            failed,
            ..
        } => {
            assert_eq!(provider.meta.status().as_u16(), 503);
            assert_eq!(provider.meta.headers()["retry-after"], "2");
            assert_eq!(
                provider.document.as_ref().unwrap().errors[0].code,
                "SERVICE_UNAVAILABLE"
            );
            assert_eq!(
                provider.meta.operation().map(|op| op.name()),
                Some("Square.payments.list")
            );
            failed
        }
        other => panic!("expected a provider page failure, got {other:?}"),
    };
    assert_eq!(resume, Resume::At(C1.to_owned()));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(
        server.requests().len(),
        before + 2,
        "page 1, failed page 2, nothing retried"
    );

    let mut resumed = api.payments().list().set_limit(2u32).pages(resume, ten);
    while let Some(page) = resumed.next_page().await? {
        kept.extend(page.items);
    }
    assert_eq!(ids(&kept), ids(&seen));
    let resumed_queries: Vec<_> = server.requests()[before + 2..]
        .iter()
        .map(Recorded::query_pairs)
        .collect();
    assert_eq!(
        resumed_queries,
        [
            vec![pair("cursor", C1), pair("limit", "2")],
            vec![pair("cursor", C2), pair("limit", "2")]
        ]
    );

    // 6. The collecting form returns the prefix with the error.
    fail_c1.store(1, Ordering::SeqCst);
    let incomplete = api
        .payments()
        .list()
        .pages(Resume::Start, ten)
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].items.len(), 2);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(C1.to_owned())));
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Provider(_))
    ));
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn workflow_2_empty_final_cursor_completes() -> Result {
    let server =
        TestServer::start(|request: &Recorded| listing_reply(request, &AtomicUsize::new(0), true))
            .await;
    let api = client(&server.url(), Duration::from_secs(5))?;
    let pages = api
        .payments()
        .list()
        .pages(Resume::Start, NonZeroUsize::new(10).unwrap());
    let all = pages
        .collect_complete()
        .await
        .map_err(|incomplete| incomplete.error)?;
    assert_eq!(
        all.iter().map(|page| page.items.len()).collect::<Vec<_>>(),
        [2, 0, 1]
    );
    assert_eq!(server.requests().len(), 3);
    assert!(
        server.requests()[0].query_pairs().is_empty(),
        "no limit unless the caller sets one"
    );
    Ok(())
}

// ---------------------------------------------------------------- failure shapes

#[tokio::test]
async fn predicted_failure_shapes() -> Result {
    let server = TestServer::start(|request: &Recorded| {
        let key = serde_json::from_slice::<Value>(&request.body)
            .ok()
            .and_then(|body| body["idempotency_key"].as_str().map(str::to_owned));
        match (request.path(), key.as_deref()) {
            ("/v2/payments", Some("fault-500")) => {
                square_error(500, "API_ERROR", "INTERNAL_SERVER_ERROR", "boom", None)
            }
            ("/v2/payments", Some("fault-502-html")) => {
                Reply::text(502, "text/html", "<html><body>Bad Gateway</body></html>")
            }
            ("/v2/payments", Some("fault-200-malformed")) => Reply::json(200, r#"{"payment":{"id":"R2B3","amou"#),
            ("/v2/payments/unauthorized", _) => square_error(
                401,
                "AUTHENTICATION_ERROR",
                "UNAUTHORIZED",
                "This request could not be authorized.",
                None,
            ),
            ("/v2/payments/html-200", _) => Reply::text(200, "text/html", "<html>login</html>"),
            ("/v2/payments/unknown-code", _) => Reply::json(
                429,
                r#"{"errors":[{"category":"RATE_LIMIT_ERROR","code":"RATE_LIMITED"},{"category":"FUTURE_CATEGORY","code":"FUTURE_CODE","detail":"x"}]}"#,
            ),
            ("/v2/payments", None) if request.method == "GET" => {
                Reply::json(200, r#"{"payments":[{"id":"GQTF","amount_money":"2500"}],"cursor":"c"}"#)
            }
            _ => Reply::new(418),
        }
    })
    .await;
    let api = client(&server.url(), Duration::from_secs(5))?;

    // Credentials and base URLs are checked locally.
    assert_eq!(bearer("").err(), Some(CredentialError::Token));
    assert_eq!(bearer("tok en").err(), Some(CredentialError::Token));
    assert_eq!(bearer("tok\nen").err(), Some(CredentialError::Token));
    let bad_base =
        client("http://127.0.0.1:1/?x=1", Duration::from_secs(1)).expect_err("query in base");
    let bad_base = bad_base
        .downcast::<concord_sdk_square_payments::concord::Error>()
        .expect("a Concord error");
    assert_eq!(bad_base.kind(), ErrorKind::Build);

    let create = |key: &str| {
        api.payments()
            .create(CreatePayment::new(key, SOURCE, usd(100)).unwrap())
            .send()
    };

    // 5xx on create: one attempt, outcome unknown, typed document.
    let before = server.requests().len();
    let error = create("fault-500").await.unwrap_err();
    assert_eq!(server.requests().len(), before + 1, "never retried");
    assert!(error.create_outcome_unknown());
    let Error::Provider(provider) = &error else {
        panic!("500 is a provider error")
    };
    assert_eq!(provider.meta.status().as_u16(), 500);
    assert_eq!(
        provider.document.as_ref().unwrap().errors[0].code,
        "INTERNAL_SERVER_ERROR"
    );
    assert!(!provider.is_idempotency_mismatch());

    // A proxy's HTML 502: status kept, document is a Concord error.
    let error = create("fault-502-html").await.unwrap_err();
    assert!(error.create_outcome_unknown());
    let Error::Provider(provider) = &error else {
        panic!("502 is a provider error")
    };
    assert_eq!(provider.meta.status().as_u16(), 502);
    let document_error = provider
        .document
        .as_ref()
        .expect_err("HTML is not a Square document");
    assert_eq!(document_error.kind(), ErrorKind::Metadata);

    // An unreadable success on create may have charged.
    let error = create("fault-200-malformed").await.unwrap_err();
    assert!(error.create_outcome_unknown());
    let Error::Concord(concord) = &error else {
        panic!("decode is a Concord error")
    };
    assert_eq!(concord.kind(), ErrorKind::Decode);
    assert_eq!(concord.head().map(|head| head.status().as_u16()), Some(200));

    // Authentication failure on a read: typed, strings withheld from diagnostics.
    let error = api.payments().get("unauthorized").send().await.unwrap_err();
    assert!(!error.create_outcome_unknown());
    let Error::Provider(provider) = &error else {
        panic!("401 is a provider error")
    };
    assert_eq!(provider.meta.status().as_u16(), 401);
    let entry = &provider.document.as_ref().unwrap().errors[0];
    assert_eq!(
        (entry.category.as_str(), entry.code.as_str()),
        ("AUTHENTICATION_ERROR", "UNAUTHORIZED")
    );
    assert_redacted(
        &format!("{error:?} {error}"),
        &[TOKEN, "could not be authorized", "unauthorized"],
    );

    // Wrong media type on a success is refused before the body is read.
    let error = api.payments().get("html-200").send().await.unwrap_err();
    let Error::Concord(concord) = &error else {
        panic!("metadata is a Concord error")
    };
    assert_eq!(concord.kind(), ErrorKind::Metadata);

    // Unknown categories/codes and several entries are retained.
    let error = api.payments().get("unknown-code").send().await.unwrap_err();
    let Error::Provider(provider) = &error else {
        panic!("429 is a provider error")
    };
    let codes: Vec<_> = provider
        .document
        .as_ref()
        .unwrap()
        .errors
        .iter()
        .map(|e| e.code.as_str())
        .collect();
    assert_eq!(codes, ["RATE_LIMITED", "FUTURE_CODE"]);

    // A malformed listing page fails the traversal at that page, nothing is retried.
    let before = server.requests().len();
    let mut pages = api
        .payments()
        .list()
        .pages(Resume::Start, NonZeroUsize::new(10).unwrap());
    match pages.next_page().await {
        Err(PagesError::Provider {
            error: Error::Concord(concord),
            failed,
            ..
        }) => {
            assert_eq!(concord.kind(), ErrorKind::Decode);
            assert_eq!(failed, Resume::Start);
        }
        other => panic!("expected a decode failure of page 1, got {other:?}"),
    }
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), before + 1);
    server.shutdown().await;
    Ok(())
}
