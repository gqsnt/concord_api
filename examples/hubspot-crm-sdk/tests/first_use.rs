//! Independent consumer tests written from the README and public rustdoc.
//! All credentials and contacts are synthetic; all network traffic is loopback.

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Gate, Recorded, Reply, TestServer, paused};
use concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_hubspot_crm::{CreateContact, Error, Filter, FilterGroup, HubSpot, SearchRequest};
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

const CLIENT: &str = "fixture-client-anna";
const SECRET: &str = "fixture-secret +&=/";
const ORIGINAL: &str = "fixture-refresh-original +&=/";
const ROTATED: &str = "fixture-refresh-replacement +&=/";
const ACCESS: &str = "fixture-access-rotated";
const TOKEN_PATH: &str = "/oauth/2026-09/token";
const CONTACTS: &str = "/crm/v3/objects/contacts";
const SEARCH: &str = "/crm/v3/objects/contacts/search";
const BODY_SECRET: &str = "fixture-private-provider-detail-anna.smith@example.com";

fn client(server: &TestServer, timeout: Duration) -> HubSpot {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(timeout)
        .build()
        .unwrap();
    HubSpot::new(http, CLIENT.into(), SECRET.into(), ORIGINAL.into())
        .unwrap()
        .with_base_url(&server.url())
        .unwrap()
}

fn response(status: u16, value: Value) -> Reply {
    Reply::json(status, value.to_string())
}

fn token() -> Reply {
    response(
        200,
        json!({
            "access_token": ACCESS, "refresh_token": ROTATED,
            "expires_in": 1800, "token_type": "bearer"
        }),
    )
}

fn contact(id: &str, name: &str) -> Value {
    json!({"id": id, "properties": {
        "firstname": name, "lastname": "Smith",
        "email": "anna.smith@example.com", "phone": null
    }, "createdAt": "2026-11-01T10:00:00Z", "archived": false})
}

fn search_request() -> SearchRequest {
    SearchRequest {
        query: Some("Anna".into()),
        filter_groups: vec![FilterGroup {
            filters: vec![Filter {
                property_name: "lastname".into(),
                operator: "EQ".into(),
                value: "Smith".into(),
            }],
        }],
        properties: ["email", "firstname", "lastname", "phone"]
            .map(String::from)
            .into(),
        limit: Some(2),
        // The traversal's Resume, not this field, owns continuation.
        after: Some("caller-value-must-be-overridden".into()),
    }
}

fn assert_oauth(request: &Recorded, refresh: &str) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, TOKEN_PATH);
    assert_eq!(request.query(), None);
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    let form: std::collections::BTreeMap<_, _> = request.form_pairs().into_iter().collect();
    assert_eq!(form.len(), 4);
    // Boolean assertions avoid printing even these synthetic credentials on failure.
    assert!(form.get("client_id").is_some_and(|s| s == CLIENT));
    assert!(form.get("client_secret").is_some_and(|s| s == SECRET));
    assert!(form.get("refresh_token").is_some_and(|s| s == refresh));
    assert_eq!(
        form.get("grant_type").map(String::as_str),
        Some("refresh_token")
    );
}

fn assert_crm(request: &Recorded, method: &str, path: &str, access: &str) {
    assert_eq!(request.method, method);
    assert_eq!(request.target, path);
    assert_eq!(request.query(), None);
    assert!(request.header("authorization") == Some(format!("Bearer {access}").as_str()));
    assert_eq!(request.header("accept"), Some("application/json"));
    if method == "POST" {
        assert_eq!(request.header("content-type"), Some("application/json"));
    } else {
        assert!(request.body.is_empty());
        assert_eq!(request.header("content-type"), None);
    }
}

fn assert_redacted(value: &impl std::fmt::Debug) {
    let debug = format!("{value:?}");
    for secret in [
        CLIENT,
        SECRET,
        ORIGINAL,
        ROTATED,
        ACCESS,
        BODY_SECRET,
        "anna.smith@example.com",
    ] {
        assert!(!debug.contains(secret), "Debug exposed a fixture secret");
    }
}

fn assert_error_redacted(error: &Error) {
    assert_redacted(error);
    let display = error.to_string();
    for secret in [
        CLIENT,
        SECRET,
        ORIGINAL,
        ROTATED,
        ACCESS,
        BODY_SECRET,
        "anna.smith@example.com",
    ] {
        assert!(
            !display.contains(secret),
            "Display exposed a fixture secret"
        );
    }
}

fn concord_error(error: &Error, kind: ErrorKind, status: u16) -> &concord::Error {
    assert_error_redacted(error);
    let Error::Concord(inner) = error else {
        panic!("expected Concord error: {error}")
    };
    assert_eq!(inner.kind(), kind);
    assert_eq!(inner.head().unwrap().status().as_u16(), status);
    if kind == ErrorKind::Status {
        assert_eq!(inner.status().unwrap().as_u16(), status);
        assert!(inner.response().is_some());
    } else {
        assert!(inner.status().is_none());
    }
    inner
}

#[tokio::test]
async fn caller_search_follow_paging_then_retrieve() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        TOKEN_PATH => token(),
        SEARCH => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            match body.get("after").and_then(Value::as_str) {
                None => response(200, json!({
                    "total": 2, "results": [contact("80101", "Anna")],
                    "paging": {"next": {"after": "002", "link": "http://127.0.0.1:1/never-follow"}}
                })),
                Some("002") => response(200, json!({
                    "total": 3, "results": [], "paging": {"next": {"after": "004"}}
                })),
                Some("004") => response(200, json!({
                    "total": 3, "results": [contact("80102", "Annabelle")]
                })),
                _ => panic!("unexpected search continuation"),
            }
        }
        "/crm/v3/objects/contacts/80101" => response(200, contact("80101", "Anna")),
        _ => panic!("unexpected fixture route"),
    }).await;
    let sdk = client(&server, Duration::from_secs(5));
    let mut pages = sdk
        .search(search_request(), Resume::Start)
        .max_pages(NonZeroUsize::new(3).unwrap());

    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.meta, 2);
    assert_eq!(first.next.as_deref(), Some("002"));
    assert_eq!(first.items[0].id, "80101");
    assert_eq!(first.items[0].properties.get("phone"), Some(&None));
    let selected = first.items[0].clone();
    let empty = pages.next_page().await.unwrap().unwrap();
    assert!(empty.items.is_empty());
    assert_eq!(empty.meta, 3);
    assert_eq!(empty.next.as_deref(), Some("004"));
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].id, "80102");
    assert_eq!(last.meta, 3);
    assert!(last.next.is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    assert!(pages.next_page().await.unwrap().is_none());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 3);
    assert_eq!(report.progress.succeeded, 3);
    assert_eq!(report.progress.failed, 0);
    assert!(report.resume.is_none());
    assert!(report.failed.is_none());
    assert_eq!(sdk.get(&selected.id).await.unwrap(), selected);
    assert!(sdk.refresh_token().unwrap() == ROTATED);

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_oauth(&requests[0], ORIGINAL);
    for (request, cursor) in requests[1..4].iter().zip([None, Some("002"), Some("004")]) {
        assert_crm(request, "POST", SEARCH, ACCESS);
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["query"], "Anna");
        assert_eq!(body["limit"], 2);
        assert_eq!(
            body["properties"],
            json!(["email", "firstname", "lastname", "phone"])
        );
        assert_eq!(
            body["filterGroups"],
            json!([{"filters": [{
                "propertyName": "lastname", "operator": "EQ", "value": "Smith"
            }]}])
        );
        assert_eq!(body.get("after").and_then(Value::as_str), cursor);
    }
    assert_crm(&requests[4], "GET", &format!("{CONTACTS}/80101"), ACCESS);
    assert_redacted(&sdk);
    server.shutdown().await;

    search_partial_results_and_resume().await;
    search_rejects_bad_pages().await;
    concurrent_refresh_keeps_latest_token().await;
    refresh_failures_are_explicit().await;
}

async fn search_partial_results_and_resume() {
    let second_attempts = Arc::new(AtomicUsize::new(0));
    let attempts = second_attempts.clone();
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        TOKEN_PATH => token(),
        SEARCH => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            if body.get("after").is_none() {
                response(
                    200,
                    json!({"total": 2, "results": [contact("80101", "Anna")],
                    "paging": {"next": {"after": "002"}}}),
                )
            } else if body["query"] == "Anna" && attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                response(
                    429,
                    json!({"status": "error", "category": "RATE_LIMITS",
                    "message": BODY_SECRET, "correlationId": "fixture-rate-429"}),
                )
                .header("retry-after", "2")
                .header("x-hubspot-correlation-id", "fixture-rate-429")
            } else {
                response(
                    200,
                    json!({"total": 2, "results": [contact("80102", "Annabelle")]}),
                )
            }
        }
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server, Duration::from_secs(5));
    let incomplete = sdk
        .search(search_request(), Resume::Start)
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "80101");
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);
    assert_eq!(incomplete.error.resume(), Some(&Resume::At("002".into())));
    assert_eq!(incomplete.error.progress().admitted, 2);
    assert_eq!(incomplete.error.progress().succeeded, 1);
    assert_eq!(incomplete.error.progress().failed, 1);
    assert_redacted(&incomplete);
    let inner = concord_error(
        incomplete.error.provider_error().unwrap(),
        ErrorKind::Status,
        429,
    );
    assert_eq!(inner.head().unwrap().headers()["retry-after"], "2");
    assert_eq!(
        inner.head().unwrap().headers()["x-hubspot-correlation-id"],
        "fixture-rate-429"
    );
    assert_eq!(server.requests().len(), 3); // No automatic retry after Retry-After.
    let resume = incomplete.error.resume().unwrap().clone();
    let remaining = sdk
        .search(search_request(), resume)
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    let mut all = incomplete.collected;
    all.extend(remaining);
    assert_eq!(
        all.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["80101", "80102"]
    );
    assert_eq!(second_attempts.load(Ordering::SeqCst), 2);

    let mut capped_request = search_request();
    capped_request.query = Some("Ann".into());
    let bounded = sdk
        .search(capped_request.clone(), Resume::Start)
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(bounded.collected.len(), 1);
    assert_eq!(bounded.error.outcome(), Outcome::Bounded);
    assert!(bounded.error.provider_error().is_none());
    assert_eq!(bounded.error.progress().admitted, 1);
    assert_eq!(bounded.error.progress().failed, 0);
    let tail = sdk
        .search(capped_request, bounded.error.resume().unwrap().clone())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(tail[0].id, "80102");
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    assert_oauth(&requests[0], ORIGINAL);
    for request in &requests[1..] {
        assert_crm(request, "POST", SEARCH, ACCESS);
    }
    // Explicit resumption reuses the same query, filters, page size, and continuation.
    assert_eq!(requests[2].body, requests[3].body);
    let mut first_body: Value = serde_json::from_slice(&requests[4].body).unwrap();
    first_body["after"] = json!("002");
    assert_eq!(
        first_body,
        serde_json::from_slice::<Value>(&requests[5].body).unwrap()
    );
    server.shutdown().await;
}

async fn search_rejects_bad_pages() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        TOKEN_PATH => token(),
        SEARCH => {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            if body.get("after").is_none() {
                return response(
                    200,
                    json!({"total": 2, "results": [contact("80101", "Anna")],
                    "paging": {"next": {"after": "002"}}}),
                );
            }
            let reply = match body["query"].as_str().unwrap() {
                "bad-json" => Reply::json(200, format!("{{\"private\":\"{BODY_SECRET}\",")),
                "bad-media" => Reply::text(200, "text/html", BODY_SECRET),
                "too-large" => Reply::json(200, " ".repeat(1024 * 1024 + 1)),
                "provider-envelope" => {
                    response(200, json!({"status":"error", "message": BODY_SECRET}))
                }
                "status-error" => response(503, json!({"status":"error", "message": BODY_SECRET})),
                "repeat" => response(
                    200,
                    json!({"total": 2, "results": [contact("80102", "Anna")],
                    "paging": {"next": {"after": "002"}}}),
                ),
                "empty-cursor" => response(
                    200,
                    json!({"total": 2, "results": [contact("80102", "Anna")],
                    "paging": {"next": {"after": ""}}}),
                ),
                _ => panic!("unexpected failure scenario"),
            };
            reply.header("x-hubspot-correlation-id", "fixture-bad-page")
        }
        "/crm/v3/objects/contacts/expired" => response(401, json!({"message": BODY_SECRET})),
        "/crm/v3/objects/contacts/missing" => response(404, json!({"message": BODY_SECRET})),
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server, Duration::from_secs(5));
    for (query, expected) in [
        ("bad-json", Some((ErrorKind::Decode, 200))),
        ("bad-media", Some((ErrorKind::Metadata, 200))),
        ("too-large", Some((ErrorKind::Limit, 200))),
        ("provider-envelope", Some((ErrorKind::Decode, 200))),
        ("status-error", Some((ErrorKind::Status, 503))),
        ("repeat", None),
        ("empty-cursor", None),
    ] {
        let before = server.requests().len();
        let mut request = search_request();
        request.query = Some(query.into());
        let mut pages = sdk
            .search(request, Resume::Start)
            .max_pages(NonZeroUsize::new(3).unwrap());
        let prefix = pages.next_page().await.unwrap().unwrap();
        assert_eq!(prefix.items[0].id, "80101");
        let failure = pages.next_page().await.unwrap_err();
        let error = failure.provider_error().unwrap();
        if let Some((kind, status)) = expected {
            let inner = concord_error(error, kind, status);
            assert_eq!(
                inner.head().unwrap().headers()["x-hubspot-correlation-id"],
                "fixture-bad-page"
            );
        } else {
            assert!(matches!(error, Error::Invalid(_)));
            assert_error_redacted(error);
        }
        assert_eq!(failure.resume(), Some(&Resume::At("002".into())));
        assert_eq!(failure.progress().succeeded, 1);
        assert_eq!(failure.progress().failed, 1);
        assert!(matches!(
            pages.next_page().await.unwrap_err(),
            PagesError::Ended { .. }
        ));
        let report = pages.finish();
        assert_eq!(report.outcome, Outcome::Failed);
        assert_eq!(report.progress.admitted, 2);
        assert_eq!(report.resume, Some(Resume::At("002".into())));
        assert_eq!(report.failed, report.resume);
        assert_eq!(
            server.requests().len() - before,
            if before == 0 { 3 } else { 2 }
        );
    }
    for (id, status) in [("expired", 401), ("missing", 404)] {
        let before = server.requests().len();
        concord_error(&sdk.get(id).await.unwrap_err(), ErrorKind::Status, status);
        assert_eq!(server.requests().len(), before + 1); // No 401 refresh/replay.
    }
    assert_eq!(
        server
            .requests()
            .iter()
            .filter(|r| r.path() == TOKEN_PATH)
            .count(),
        1
    );
    server.shutdown().await;
}

async fn concurrent_refresh_keeps_latest_token() {
    const NEWEST: &str = "fixture-refresh-newest";
    const SECOND_ACCESS: &str = "fixture-access-second-generation";
    let gates = [Gate::new(), Gate::new()];
    let reply_gates = gates.clone();
    let exchanges = Arc::new(AtomicUsize::new(0));
    let seen = exchanges.clone();
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        TOKEN_PATH => {
            let generation = seen.fetch_add(1, Ordering::SeqCst);
            assert!(generation < 2, "duplicate concurrent token exchange");
            let (access, refresh, lifetime) = if generation == 0 {
                (ACCESS, ROTATED, 31)
            } else {
                (SECOND_ACCESS, NEWEST, 1800)
            };
            response(
                200,
                json!({"access_token": access, "refresh_token": refresh,
                "expires_in": lifetime, "token_type": "bearer"}),
            )
            .after(reply_gates[generation].clone())
        }
        SEARCH => response(
            200,
            json!({"total": 1, "results": [contact("80101", "Anna")]}),
        ),
        "/crm/v3/objects/contacts/80101" => response(200, contact("80101", "Anna")),
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server, Duration::from_secs(5));
    let follower_sdk = sdk.clone();
    for (generation, gate) in gates.iter().enumerate() {
        if generation == 1 {
            // Documented 30-second refresh margin; no private clock/cache manipulation.
            tokio::time::sleep(Duration::from_millis(1100)).await;
        }
        let leader_sdk = sdk.clone();
        let leader =
            tokio::spawn(async move { leader_sdk.search_page(search_request(), None).await });
        assert!(
            server
                .wait_until(Duration::from_secs(2), |_| {
                    exchanges.load(Ordering::SeqCst) == generation + 1
                })
                .await
        );
        let mut follower = Box::pin(follower_sdk.get("80101"));
        // Poll a second caller while the OAuth response is held. This is actual overlap,
        // not two calls which happened to reuse an already cached access token.
        poll_fn(|cx| {
            assert!(follower.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        assert_eq!(exchanges.load(Ordering::SeqCst), generation + 1);
        assert_eq!(
            server
                .requests()
                .iter()
                .filter(|r| r.path() != TOKEN_PATH)
                .count(),
            generation * 2
        );
        gate.open();
        let (page, found) = tokio::join!(leader, follower);
        assert_eq!(page.unwrap().unwrap().items[0].id, found.unwrap().id);
        assert!(sdk.refresh_token().unwrap() == if generation == 0 { ROTATED } else { NEWEST });
        assert!(follower_sdk.refresh_token().unwrap() == sdk.refresh_token().unwrap());
    }
    // A subsequent user of an old clone must not overwrite the newest replacement.
    assert_eq!(follower_sdk.get("80101").await.unwrap().id, "80101");
    assert!(sdk.refresh_token().unwrap() == NEWEST);
    assert_eq!(exchanges.load(Ordering::SeqCst), 2);
    let requests = server.requests();
    assert_eq!(requests.len(), 7);
    assert_oauth(&requests[0], ORIGINAL);
    assert_oauth(&requests[3], ROTATED);
    for (index, access) in [
        (1, ACCESS),
        (2, ACCESS),
        (4, SECOND_ACCESS),
        (5, SECOND_ACCESS),
        (6, SECOND_ACCESS),
    ] {
        let request = &requests[index];
        assert_crm(request, &request.method, request.path(), access);
    }
    assert_redacted(&sdk);
    assert!(!format!("{sdk:?}").contains(NEWEST));
    assert!(!format!("{sdk:?}").contains(SECOND_ACCESS));
    server.shutdown().await;
}

async fn refresh_failures_are_explicit() {
    for (scenario, status, uncertain) in [
        ("400", Some(400), false),
        ("401", Some(401), false),
        ("403", Some(403), false),
        ("503", Some(503), true),
        ("malformed", None, true),
        ("missing-token-type", None, true),
        ("short-lifetime", None, true),
    ] {
        let server = TestServer::start(move |request: &Recorded| {
            assert_eq!(
                request.path(),
                TOKEN_PATH,
                "CRM must not dispatch without a token"
            );
            match scenario {
                "malformed" => Reply::json(200, format!("{{\"private\":\"{BODY_SECRET}\",")),
                "missing-token-type" => {
                    response(200, json!({"access_token": ACCESS, "expires_in": 1800}))
                }
                "short-lifetime" => response(
                    200,
                    json!({"access_token": ACCESS,
                    "refresh_token": ROTATED, "expires_in": 30, "token_type": "bearer"}),
                ),
                _ => response(
                    status.unwrap(),
                    json!({"error": "invalid_grant", "error_description": BODY_SECRET}),
                ),
            }
        })
        .await;
        let sdk = client(&server, Duration::from_secs(5));
        let error = sdk.get("80101").await.unwrap_err();
        assert_error_redacted(&error);
        assert!(
            matches!(error, Error::Refresh { status: actual, uncertain: flag }
            if actual == status && flag == uncertain),
            "unexpected refresh classification: {error}"
        );
        if uncertain {
            let repeated = sdk.get("80101").await.unwrap_err();
            // Observed public behavior: later operations erase the original status,
            // while refresh_token() below uses the dedicated RefreshUncertain variant.
            assert!(
                matches!(
                    repeated,
                    Error::Refresh {
                        status: None,
                        uncertain: true
                    }
                ),
                "scenario {scenario}: {repeated:?}"
            );
            assert_error_redacted(&repeated);
            assert!(matches!(
                sdk.refresh_token().unwrap_err(),
                Error::RefreshUncertain
            ));
        } else {
            assert!(sdk.refresh_token().unwrap() == ORIGINAL);
        }
        assert_oauth(&server.single(), ORIGINAL);
        server.shutdown().await;
    }
}

fn create_input() -> CreateContact {
    let mut input = CreateContact::default();
    for (name, value) in [
        ("email", "anna.smith@example.com"),
        ("firstname", "Anna"),
        ("lastname", "Smith"),
        ("company", "Anna Smith's Bakery"),
    ] {
        input.properties.insert(name.into(), value.into());
    }
    input
}

#[tokio::test]
async fn caller_create_then_later_lookup_by_id() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        // A refresh without a replacement is valid and preserves the original token.
        TOKEN_PATH => response(
            200,
            json!({"access_token": ACCESS, "expires_in": 1800, "token_type": "bearer"}),
        ),
        CONTACTS => response(201, contact("90201", "Anna"))
            .header("x-hubspot-correlation-id", "fixture-created"),
        "/crm/v3/objects/contacts/90201" => response(200, contact("90201", "Anna")),
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let sdk = client(&server, Duration::from_secs(5));
    let input = create_input();
    let expected = serde_json::to_value(&input).unwrap();
    let created = sdk.create(input).await.unwrap();
    assert_eq!(created.id, "90201");
    assert_eq!(
        created.properties["email"].as_deref(),
        Some("anna.smith@example.com")
    );
    // Store the returned ID, then use it in the later read step.
    let saved_id = created.id.clone();
    assert_eq!(sdk.get(&saved_id).await.unwrap(), created);
    assert!(sdk.refresh_token().unwrap() == ORIGINAL);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_oauth(&requests[0], ORIGINAL);
    assert_crm(&requests[1], "POST", CONTACTS, ACCESS);
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[1].body).unwrap(),
        expected
    );
    assert_eq!(
        expected,
        json!({"properties": {"email": "anna.smith@example.com",
        "firstname": "Anna", "lastname": "Smith", "company": "Anna Smith's Bakery"}})
    );
    assert_crm(
        &requests[2],
        "GET",
        &format!("{CONTACTS}/{saved_id}"),
        ACCESS,
    );
    server.shutdown().await;

    create_timeout_remains_uncertain().await;
    create_failures_are_not_replayed().await;
}

async fn create_timeout_remains_uncertain() {
    let accepted = Arc::new(AtomicUsize::new(0));
    let created_count = accepted.clone();
    let server = TestServer::start(move |request: &Recorded| match request.path() {
        TOKEN_PATH => token(),
        CONTACTS => {
            created_count.fetch_add(1, Ordering::SeqCst);
            // The provider has committed the write; the successful reply never arrives.
            response(201, contact("90202", "Anna")).after(Gate::new())
        }
        "/crm/v3/objects/contacts/90202" => {
            assert_eq!(created_count.load(Ordering::SeqCst), 1);
            response(200, contact("90202", "Anna"))
        }
        _ => panic!("unexpected fixture route"),
    })
    .await;
    let timeout = Duration::from_millis(200);
    let sdk = client(&server, timeout);
    // Paused clock: the token request, the delivery of the create and the reconciling read
    // complete in frozen time; the 200 ms timeout fires only once the server holds the create.
    tokio::time::pause();
    let create = paused::time_out(
        &server,
        |r| r.path() == CONTACTS,
        timeout,
        sdk.create(create_input()),
    );
    let error = tokio::time::timeout(Duration::from_secs(3), create)
        .await
        .expect("caller timeout must be bounded")
        .unwrap_err();
    assert!(matches!(error, Error::CreateUncertain));
    assert_error_redacted(&error);
    assert_eq!(accepted.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 2);
    // This ID is independent fixture knowledge; CreateUncertain supplied no ID.
    // The real application would need its own reconciliation strategy before retrying.
    let reconciled = paused::drive(sdk.get("90202")).await.unwrap();
    tokio::time::resume();
    assert_eq!(reconciled.id, "90202");
    assert_eq!(
        reconciled.properties["email"].as_deref(),
        Some("anna.smith@example.com")
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_oauth(&requests[0], ORIGINAL);
    assert_crm(&requests[1], "POST", CONTACTS, ACCESS);
    assert_crm(&requests[2], "GET", &format!("{CONTACTS}/90202"), ACCESS);
    assert_eq!(accepted.load(Ordering::SeqCst), 1); // No second create during reconciliation.
    server.shutdown().await;
}

async fn create_failures_are_not_replayed() {
    for (scenario, status, uncertain) in [
        ("conflict", 409, false),
        ("request-timeout", 408, true),
        ("unavailable", 503, true),
        ("malformed-success", 201, true),
        ("wrong-media", 201, true),
        ("disconnect", 0, true),
    ] {
        let server = TestServer::start(move |request: &Recorded| match request.path() {
            TOKEN_PATH => token(),
            CONTACTS => match scenario {
                "malformed-success" => {
                    Reply::json(status, format!("{{\"private\":\"{BODY_SECRET}\","))
                }
                "wrong-media" => Reply::text(status, "text/html", BODY_SECRET),
                "disconnect" => Reply::hang_up(),
                _ => response(
                    status,
                    json!({"status": "error", "message": BODY_SECRET,
                    "category": "CONFLICT", "correlationId": "fixture-create-error"}),
                )
                .header("x-hubspot-correlation-id", "fixture-create-error"),
            },
            _ => panic!("unexpected fixture route"),
        })
        .await;
        let sdk = client(&server, Duration::from_secs(5));
        let error = sdk.create(create_input()).await.unwrap_err();
        assert_error_redacted(&error);
        if uncertain {
            assert!(matches!(error, Error::CreateUncertain));
        } else {
            let inner = concord_error(&error, ErrorKind::Status, status);
            assert_eq!(
                inner.head().unwrap().headers()["x-hubspot-correlation-id"],
                "fixture-create-error"
            );
        }
        let requests = server.requests();
        assert_eq!(requests.len(), 2);
        assert_oauth(&requests[0], ORIGINAL);
        assert_crm(&requests[1], "POST", CONTACTS, ACCESS);
        server.shutdown().await;
    }
}
