//! Connected contacts workflows and bounded provider pagination against local fixtures.

mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer, paused};
use concord_sdk_intercom::{
    Error, HeaderNumber, NewContact, PageSize, Role,
    pagination::{PagesError, Resume},
};
use support::*;

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn list_provider_pages_then_retrieve_one_contact() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (
            request.method.as_str(),
            request.path(),
            request.query_all("starting_after").as_slice(),
        ) {
            ("GET", "/contacts", []) => Reply::json(200, LIST_FIRST),
            ("GET", "/contacts", [cursor]) if cursor == "cursor+/=?&fixture" => {
                Reply::json(200, LIST_LAST)
            }
            ("GET", "/contacts/111111111111111111111111", []) => Reply::json(200, FIRST),
            _ => Reply::new(404),
        }
    })
    .await;
    let api = api(&server);
    let contacts = api
        .contacts()
        .list()
        .set_per_page(PageSize::new(1).unwrap())
        .pages(Resume::Start, cap(3))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        contacts.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["111111111111111111111111", "222222222222222222222222"]
    );
    let one = api.contacts().get(&contacts[0].id).send().await.unwrap();
    assert_eq!(one.value.email.as_deref(), Some("first@example.test"));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].target, "/contacts?per_page=1");
    assert_eq!(
        requests[1].target,
        "/contacts?per_page=1&starting_after=cursor%2B%2F%3D%3F%26fixture"
    );
    for request in requests {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer SYNTHETIC_SECRET")
        );
        assert_eq!(request.header("intercom-version"), Some("2.17"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn create_contact_then_retrieve_returned_identifier() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        match (request.method.as_str(), request.path()) {
            ("POST", "/contacts") => {
                let actual: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                let expected: serde_json::Value =
                    serde_json::from_str(include_str!("../fixtures/create-email.json")).unwrap();
                assert_eq!(actual, expected);
                Reply::json(200, CREATED)
            }
            ("GET", "/contacts/222222222222222222222222") => Reply::json(200, CREATED),
            _ => Reply::new(404),
        }
    })
    .await;
    let api = api(&server);
    let mut input = NewContact::with_email("created@example.test");
    input.name = Some("Created Fixture".into());
    input.role = Some(Role::Lead);
    let created = api.contacts().create(input).send().await.unwrap().value;
    let retrieved = api.contacts().get(&created.id).send().await.unwrap().value;
    assert_eq!(retrieved.id, created.id);
    assert_eq!(retrieved.email.as_deref(), Some("created@example.test"));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].header("content-type"), Some("application/json"));
    assert_eq!(requests[0].header("intercom-version"), Some("2.17"));
    assert_eq!(
        requests[1].header("authorization"),
        Some("Bearer SYNTHETIC_SECRET")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn headerless_429_interrupts_pagination_with_prefix_and_no_reset_or_retry() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.query_all("starting_after").is_empty() {
            Reply::json(200, LIST_FIRST)
        } else {
            rate_error()
        }
    })
    .await;
    let api = api(&server);
    // Clock frozen while the call runs: the bound fails only a call that waits for something
    // that never comes (then reported as stuck).
    let result = tokio::time::timeout(
        std::time::Duration::from_millis(500),
        paused::frozen(
            api.contacts()
                .list()
                .pages(Resume::Start, cap(20))
                .items()
                .collect_complete(),
        ),
    )
    .await
    .unwrap();
    let incomplete = result.unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    let error = incomplete.error.provider_error().unwrap();
    let Error::Provider(error) = error else {
        panic!("{error}")
    };
    assert_eq!(error.meta.status().as_u16(), 429);
    assert_eq!(
        error.rate_limits().reset_unix_seconds,
        HeaderNumber::Missing
    );
    assert_eq!(
        error
            .retry_after(std::time::SystemTime::UNIX_EPOCH)
            .unwrap(),
        None
    );
    let (_, failed, _) = incomplete.error.into_provider_error().unwrap();
    assert_eq!(failed, Resume::At("cursor+/=?&fixture".into()));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_intermediate_page_continues_and_null_pages_exhausts() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.query_all("starting_after").is_empty() {
            Reply::json(200, include_str!("../fixtures/list-empty-next.json"))
        } else {
            assert_eq!(request.query_all("starting_after"), ["after-empty"]);
            Reply::json(200, include_str!("../fixtures/list-null-pages.json"))
        }
    })
    .await;
    let mut pages = api(&server).contacts().list().pages(Resume::Start, cap(2));
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().unwrap().items.is_empty());
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn page_cap_retains_resume_point_and_never_reports_completion() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(200, LIST_FIRST) }).await;
    let mut pages = api(&server).contacts().list().pages(Resume::Start, cap(1));
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 1);
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error, PagesError::Bounded { .. }));
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn explicit_resume_uses_failed_cursor_and_preserves_template_size() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.query_all("starting_after"), ["resume+/=&"]);
        assert_eq!(request.query_all("per_page"), ["150"]);
        Reply::json(200, LIST_LAST)
    })
    .await;
    let all = api(&server)
        .contacts()
        .list()
        .set_starting_after("replaced")
        .set_per_page(PageSize::new(150).unwrap())
        .pages(Resume::At("resume+/=&".into()), cap(1))
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_or_nonadvancing_cursors_stop_without_an_extra_request() {
    for (body, from) in [
        (
            include_str!("../fixtures/list-empty-cursor.json"),
            Resume::Start,
        ),
        (LIST_FIRST, Resume::At("cursor+/=?&fixture".into())),
    ] {
        let server =
            TestServer::start(move |_: &Recorded| -> Reply { Reply::json(200, body) }).await;
        let error = api(&server)
            .contacts()
            .list()
            .pages(from, cap(10))
            .next_page()
            .await
            .unwrap_err();
        assert!(matches!(
            error.provider_error(),
            Some(Error::InvalidCursor(_))
        ));
        assert_eq!(
            error.provider_error().unwrap().status().unwrap().as_u16(),
            200
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
