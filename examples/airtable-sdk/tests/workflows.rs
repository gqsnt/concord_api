#![allow(missing_docs)]
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_airtable::{
    Client, ListOptions,
    pagination::{Outcome, PagesError, Resume},
};
use futures_util::StreamExt;
use std::num::NonZeroUsize;

fn client(server: &TestServer) -> Client {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    Client::new(http, "SYNTHETIC_TOKEN_NOT_A_CREDENTIAL")
        .with_base_url(server.url())
        .unwrap()
}
fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn workflow_a_traverses_opaque_offset_and_dynamic_fields() {
    let script = Script::new([
        Reply::json(200, include_bytes!("../fixtures/page-first.json"))
            .header("x-request-id", "first"),
        Reply::json(200, include_bytes!("../fixtures/page-last.json"))
            .header("x-request-id", "last"),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let mut pages = client(&server).list_pages(
        "appDemo",
        "Tasks",
        ListOptions::default(),
        Resume::Start,
        cap(2),
    );
    assert!(server.requests().is_empty());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[0].fields["Count"], 3);
    assert_eq!(first.meta.headers()["x-request-id"], "first");
    let last = pages.next_page().await.unwrap().unwrap();
    assert_eq!(last.items[0].fields["Renamed field"], "second");
    assert_eq!(last.items[0].fields["Unknown"]["nested"], true);
    assert!(!last.items[0].fields.contains_key("Name"));
    assert!(pages.next_page().await.unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, "/v0/appDemo/Tasks");
    assert_eq!(requests[1].query_all("offset"), ["itr/rec+?&=% #"]);
    server.shutdown().await;
}

#[tokio::test]
async fn workflow_b_keeps_filters_and_selected_fields_across_pages_and_streams() {
    let script = Script::new([
        Reply::json(200, include_bytes!("../fixtures/selected-first.json")),
        Reply::json(200, include_bytes!("../fixtures/selected-last.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let options = ListOptions {
        fields: vec!["Renamed field".into(), "Unknown".into()],
        filter_by_formula: Some("{Renamed field} = 'x&y'".into()),
        ..Default::default()
    };
    let mut stream = client(&server)
        .list_pages("app", "Tasks", options, Resume::Start, cap(2))
        .items()
        .into_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().id, "recOne");
    assert_eq!(stream.next().await.unwrap().unwrap().id, "recTwo");
    assert!(stream.next().await.is_none());
    for r in server.requests() {
        assert_eq!(r.query_all("fields[]"), ["Renamed field", "Unknown"]);
        assert_eq!(r.query_all("filterByFormula"), ["{Renamed field} = 'x&y'"]);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn partial_collection_preserves_resume_on_cap_and_failure() {
    let script = Script::new([
        Reply::json(200, include_bytes!("../fixtures/page-first.json")),
        Reply::json(200, include_bytes!("../fixtures/page-first.json")),
        Reply::json(403, include_bytes!("../fixtures/error-object.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let incomplete = api
        .list_pages(
            "app",
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(1),
        )
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert!(
        matches!(incomplete.error, PagesError::Bounded { resume: Resume::At(ref offset), .. } if offset == "itr/rec+?&=% #")
    );
    let incomplete = api
        .list_pages(
            "app",
            "Tasks",
            ListOptions::default(),
            Resume::Start,
            cap(3),
        )
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 1);
    assert!(
        matches!(incomplete.error, PagesError::Provider { failed: Resume::At(ref offset), .. } if offset == "itr/rec+?&=% #")
    );
    server.shutdown().await;
}
