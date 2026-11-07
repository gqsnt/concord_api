//! Fresh caller workflows against a loopback Intercom fixture.

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_intercom::{
    Error, ErrorCode, HeaderNumber, Intercom, NewContact, PageSize, Role, http_client,
    pagination::Resume,
};
use std::{
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

const TOKEN: &str = "fixture-token-29";
const CURSOR: &str = "next+/= &opaque";
const FIRST: &str = r#"{"type":"list","data":[{"type":"contact","id":"c_ada","role":"user","email":"ada@example.test","name":"Ada Lovelace"}],"total_count":99,"pages":{"next":{"starting_after":"next+/= &opaque"}}}"#;
const EMPTY: &str =
    r#"{"type":"list","data":[],"total_count":99,"pages":{"next":{"starting_after":"last-page"}}}"#;
const LAST: &str = r#"{"type":"list","data":[{"type":"contact","id":"c_grace","role":"lead","email":"grace@example.test"}],"total_count":99,"pages":{"next":null}}"#;
const ADA: &str = r#"{"type":"contact","id":"c_ada","role":"user","email":"ada@example.test","name":"Ada Lovelace"}"#;
const CREATED: &str = r#"{"type":"contact","id":"c_new_42","role":"lead","email":"new.person@example.test","name":"New Person"}"#;

fn client(server: &TestServer) -> Intercom {
    Intercom::with_token(http_client().unwrap(), TOKEN)
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
}

fn assert_common(request: &Recorded) {
    assert_eq!(
        request.header("authorization"),
        Some("Bearer fixture-token-29")
    );
    assert_eq!(request.header("intercom-version"), Some("2.17"));
    assert_eq!(request.header("accept"), Some("application/json"));
}

#[tokio::test]
async fn list_contacts_then_retrieve_and_handle_incomplete_results() {
    let server = TestServer::start(|request: &Recorded| {
        match (
            request.path(),
            request
                .query_all("starting_after")
                .first()
                .map(String::as_str),
        ) {
            ("/contacts", None) => Reply::json(200, FIRST).header("x-request-id", "page-one"),
            ("/contacts", Some(CURSOR)) => Reply::json(200, EMPTY),
            ("/contacts", Some("last-page")) => Reply::json(200, LAST),
            ("/contacts/c_ada", None) => Reply::json(200, ADA).header("x-request-id", "get-ada"),
            _ => Reply::json(
                404,
                r#"{"type":"error.list","errors":[{"code":"not_found"}]}"#,
            ),
        }
    })
    .await;
    let api = client(&server);
    assert!(Intercom::with_token(http_client().unwrap(), "invalid token").is_err());
    assert!(PageSize::new(0).is_err());
    assert!(!format!("{api:?}").contains(TOKEN));
    let mut pages = api
        .contacts()
        .list()
        .set_per_page(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(3).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].id, "c_ada");
    assert_eq!(first.meta.total_count, 99);
    assert_eq!(first.meta.response.status().as_u16(), 200);
    assert_eq!(
        first.meta.response.headers().get("x-request-id").unwrap(),
        "page-one"
    );
    assert_eq!(first.next.as_deref(), Some(CURSOR));
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert_eq!(
        pages.next_page().await.unwrap().unwrap().items[0].id,
        "c_grace"
    );
    assert!(pages.next_page().await.unwrap().is_none());
    let got = api.contacts().get(&first.items[0].id).send().await.unwrap();
    assert_eq!(got.value.email.as_deref(), Some("ada@example.test"));
    assert_eq!(got.meta.status().as_u16(), 200);
    assert_eq!(got.meta.headers().get("x-request-id").unwrap(), "get-ada");
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for r in &requests {
        assert_common(r);
        assert_eq!(r.method, "GET");
    }
    assert_eq!(requests[0].path(), "/contacts");
    assert_eq!(requests[0].query_pairs(), [("per_page".into(), "2".into())]);
    assert_eq!(
        requests[1].query_pairs(),
        [
            ("per_page".into(), "2".into()),
            ("starting_after".into(), CURSOR.into())
        ]
    );
    assert_eq!(requests[2].query_all("starting_after"), ["last-page"]);
    assert_eq!(requests[3].path(), "/contacts/c_ada");
    assert_eq!(requests[3].query(), None);
    server.shutdown().await;

    let limited = TestServer::start(|request: &Recorded| {
        if request.query_all("starting_after").is_empty() { Reply::json(200, FIRST) }
        else { Reply::json(429, r#"{"type":"error.list","request_id":"req_429","errors":[{"code":"rate_limit_exceeded","message":"private provider message"}]}"#).header("x-request-id", "limited") }
    }).await;
    let api = client(&limited);
    let incomplete = tokio::time::timeout(
        Duration::from_secs(2),
        api.contacts()
            .list()
            .set_per_page(PageSize::new(2).unwrap())
            .pages(Resume::Start, NonZeroUsize::new(5).unwrap())
            .items()
            .collect_complete(),
    )
    .await
    .expect("must not loop or wait")
    .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "c_ada");
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    let error = incomplete.error.provider_error().unwrap();
    assert_eq!(error.status().unwrap().as_u16(), 429);
    let Error::Provider(provider) = error else {
        panic!("expected provider error")
    };
    assert_eq!(
        provider.meta.headers().get("x-request-id").unwrap(),
        "limited"
    );
    assert_eq!(
        provider.rate_limits().reset_unix_seconds,
        HeaderNumber::Missing
    );
    assert_eq!(provider.rate_limits().limit, HeaderNumber::Missing);
    assert_eq!(provider.rate_limits().remaining, HeaderNumber::Missing);
    assert_eq!(provider.retry_after(SystemTime::now()).unwrap(), None);
    assert_eq!(
        provider.document.as_ref().unwrap().errors[0].code,
        ErrorCode::Text("rate_limit_exceeded".into())
    );
    let diagnostic = format!("{error:?} {error}");
    for secret in [TOKEN, CURSOR, "private provider message", "c_ada"] {
        assert!(!diagnostic.contains(secret));
    }
    let requests = limited.requests();
    assert_eq!(requests.len(), 2);
    for r in &requests {
        assert_common(r);
        assert_eq!(r.method, "GET");
    }
    assert_eq!(requests[1].query_all("starting_after"), [CURSOR]);
    limited.shutdown().await;

    let bounded = TestServer::start(|request: &Recorded| {
        if request.query_all("starting_after").is_empty() {
            Reply::json(200, FIRST)
        } else if request.query_all("starting_after") == [CURSOR] {
            Reply::json(200, LAST)
        } else {
            Reply::json(400, "{}")
        }
    })
    .await;
    let incomplete = client(&bounded)
        .contacts()
        .list()
        .set_per_page(PageSize::new(2).unwrap())
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(
        incomplete.error.resume(),
        Some(&Resume::At(CURSOR.to_owned()))
    );
    assert!(incomplete.error.provider_error().is_none());
    assert_eq!(bounded.requests().len(), 1);
    let resumed = client(&bounded)
        .contacts()
        .list()
        .set_per_page(PageSize::new(2).unwrap())
        .pages(
            incomplete.error.resume().unwrap().clone(),
            NonZeroUsize::new(1).unwrap(),
        )
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].id, "c_grace");
    assert_eq!(bounded.requests()[1].query_all("starting_after"), [CURSOR]);
    bounded.shutdown().await;

    let malformed = TestServer::start(|_: &Recorded| {
        Reply::json(200, r#"{"type":"list","data":[],"total_count":0}"#)
    })
    .await;
    let error = client(&malformed)
        .contacts()
        .list()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Concord(_)));
    assert_eq!(error.status().unwrap().as_u16(), 200);
    assert_eq!(malformed.requests().len(), 1);
    malformed.shutdown().await;
}

#[tokio::test]
async fn create_contact_then_retrieve_and_handle_failures() {
    let server =
        TestServer::start(
            |request: &Recorded| match (request.method.as_str(), request.path()) {
                ("POST", "/contacts") => {
                    Reply::json(200, CREATED).header("x-request-id", "create-42")
                }
                ("GET", "/contacts/c_new_42") => {
                    Reply::json(200, CREATED).header("x-request-id", "get-42")
                }
                _ => Reply::json(
                    404,
                    r#"{"type":"error.list","errors":[{"code":"not_found"}]}"#,
                ),
            },
        )
        .await;
    let api = client(&server);
    let mut input = NewContact::with_email("new.person@example.test");
    input.name = Some("New Person".into());
    input.role = Some(Role::Lead);
    let created = api.contacts().create(input).send().await.unwrap();
    assert_eq!(created.value.id, "c_new_42");
    assert_eq!(created.meta.status().as_u16(), 200);
    assert_eq!(
        created.meta.headers().get("x-request-id").unwrap(),
        "create-42"
    );
    let fetched = api.contacts().get(&created.value.id).send().await.unwrap();
    assert_eq!(
        fetched.value.email.as_deref(),
        Some("new.person@example.test")
    );
    assert_eq!(fetched.value.id, created.value.id);
    assert_eq!(
        fetched.meta.headers().get("x-request-id").unwrap(),
        "get-42"
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for r in &requests {
        assert_common(r);
        assert_eq!(r.query(), None);
    }
    assert_eq!(
        (requests[0].method.as_str(), requests[0].path()),
        ("POST", "/contacts")
    );
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&requests[0].body).unwrap(),
        serde_json::json!({"email":"new.person@example.test", "name":"New Person", "role":"lead"})
    );
    assert_eq!(
        (requests[1].method.as_str(), requests[1].path()),
        ("GET", "/contacts/c_new_42")
    );
    server.shutdown().await;

    let rejected = TestServer::start(|_: &Recorded| Reply::json(422, r#"{"type":"error.list","request_id":"req_422","errors":[{"code":"parameter_invalid","message":"new.person@example.test invalid"}]}"#)).await;
    let error = client(&rejected)
        .contacts()
        .create(NewContact::with_email("new.person@example.test"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 422);
    assert!(!format!("{error:?} {error}").contains("new.person@example.test"));
    let Error::Provider(provider) = error else {
        panic!("expected provider error")
    };
    assert_eq!(
        provider.document.unwrap().errors[0].code,
        ErrorCode::Text("parameter_invalid".into())
    );
    assert_eq!(rejected.requests().len(), 1);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&rejected.single().body).unwrap(),
        serde_json::json!({"email":"new.person@example.test"})
    );
    rejected.shutdown().await;

    let bad_error = TestServer::start(|_: &Recorded| Reply::json(503, "not json")).await;
    let error = client(&bad_error)
        .contacts()
        .create(NewContact::with_email("new.person@example.test"))
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status().unwrap().as_u16(), 503);
    let Error::Provider(provider) = error else {
        panic!("expected provider error")
    };
    assert!(provider.document.is_err());
    assert_eq!(bad_error.requests().len(), 1);
    bad_error.shutdown().await;

    let lost = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        client(&lost)
            .contacts()
            .create(NewContact::with_email("new.person@example.test"))
            .send(),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(matches!(error, Error::Concord(_)));
    assert_eq!(
        lost.requests().len(),
        1,
        "ambiguous create must not be replayed"
    );
    lost.shutdown().await;
}
