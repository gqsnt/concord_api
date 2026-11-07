//! Public caller: bounded email search, explicit resumption, then retrieval by ID.

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_intercom::{
    EmailSearch, Intercom, PageSize, http_client,
    pagination::{PagesError, Resume},
};
use serde_json::{Value, json};

const EMAIL: &str = "shared+tag@example.test";
const CURSOR: &str = "next+/= &opaque";

#[tokio::test]
async fn find_all_email_matches_with_a_cap_resume_then_retrieve() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer fixture-search-token")
        );
        assert_eq!(request.header("intercom-version"), Some("2.17"));
        assert_eq!(request.query(), None);
        match (request.method.as_str(), request.path()) {
            ("POST", "/contacts/search") => {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(
                    body["query"],
                    json!({"field":"email", "operator":"=", "value":EMAIL})
                );
                assert_eq!(body["pagination"]["per_page"], 1);
                let (id, next) = match body["pagination"].get("starting_after") {
                    None => ("match-one", json!({"starting_after": CURSOR})),
                    Some(cursor) if cursor == CURSOR => ("match-two", Value::Null),
                    _ => panic!("resume must replace the template cursor"),
                };
                Reply::json(
                    200,
                    json!({"type":"list", "data":[{"type":"contact", "id":id,
                    "role":"user", "email":EMAIL}], "total_count":99, "pages":{"next":next}})
                    .to_string(),
                )
                .header("x-request-id", id)
            }
            ("GET", "/contacts/match-two") => Reply::json(
                200,
                json!({"type":"contact", "id":"match-two", "role":"user", "email":EMAIL})
                    .to_string(),
            ),
            _ => Reply::new(404),
        }
    })
    .await;
    let api = Intercom::with_token(http_client().unwrap(), "fixture-search-token")
        .unwrap()
        .with_base_url(server.url())
        .unwrap();
    let search = api.contacts().search(
        EmailSearch::new(EMAIL)
            .with_per_page(PageSize::new(1).unwrap())
            .with_starting_after(Some("stale-template-cursor".into())),
    );
    let incomplete = search
        .clone()
        .pages(Resume::Start, NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(matches!(incomplete.error, PagesError::Bounded { .. }));
    assert_eq!(incomplete.collected.len(), 1);
    assert_eq!(incomplete.collected[0].id, "match-one");
    assert_eq!(incomplete.error.resume(), Some(&Resume::At(CURSOR.into())));
    assert_eq!(
        server.requests().len(),
        1,
        "the cap must not issue a second request"
    );
    let mut resumed = search.pages(
        incomplete.error.resume().unwrap().clone(),
        NonZeroUsize::new(1).unwrap(),
    );
    let last = resumed.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].id, "match-two");
    assert_eq!(last.meta.total_count, 99);
    assert_eq!(last.meta.response.headers()["x-request-id"], "match-two");
    assert!(last.next.is_none());
    assert!(resumed.next_page().await.unwrap().is_none());
    let mut matches = incomplete.collected;
    matches.extend(last.items);
    assert_eq!(
        matches
            .iter()
            .map(|c| c.email.as_deref())
            .collect::<Vec<_>>(),
        [Some(EMAIL), Some(EMAIL)]
    );
    let retrieved = api.contacts().get(&matches[1].id).send().await.unwrap();
    assert_eq!(retrieved.value.id, matches[1].id);
    assert_eq!(retrieved.value.email.as_deref(), Some(EMAIL));
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}
