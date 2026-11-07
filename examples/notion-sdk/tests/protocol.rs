//! Wire-level protocol, model compatibility, and error-boundary tests.
use concord::{ErrorKind, bytes::Bytes, reqwest};
use concord_example_testserver::{Body, Reply, Script, TestServer};
use concord_sdk_notion::models::{BlockValue, PropertyValue, RichText};
use concord_sdk_notion::{
    Error, ErrorCode, NOTION_VERSION, NotionClient, PageSize, Query, SelectEquals, render,
};
use serde_json::{Value, json};

const PAGE: &[u8] = include_bytes!("../fixtures/page.json");
const QUERY1: &[u8] = include_bytes!("../fixtures/query-1.json");
const QUERY2: &[u8] = include_bytes!("../fixtures/query-2.json");
const CHILDREN1: &[u8] = include_bytes!("../fixtures/children-1.json");
const CHILDREN2: &[u8] = include_bytes!("../fixtures/children-2.json");
const LAST: &[u8] = include_bytes!("../fixtures/empty-last.json");

#[tokio::test]
async fn native_send_keeps_the_declared_success_body_limit() {
    let server =
        TestServer::start(|_| Reply::json(200, vec![b' '; concord_sdk_notion::RESPONSE_LIMIT + 1]))
            .await;
    let api = concord_sdk_notion::api::Notion::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        concord_sdk_notion::api::NotionConfig {
            token: "synthetic-notion-token".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let error = api
        .page("page-id")
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert_eq!(error.head().unwrap().status().as_u16(), 200);
    server.shutdown().await;
}

fn json_reply(status: u16, body: &[u8]) -> Reply {
    Reply::new(status)
        .header("Content-Type", "application/json")
        .header("x-request-id", "fixture-request-id")
        .body(body)
}
fn client(server: &TestServer) -> NotionClient {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    NotionClient::new(http, "synthetic-notion-token")
        .with_base_url(&format!("{}/v1", server.url()))
        .unwrap()
}
fn query() -> Query {
    Query {
        filter: Some(SelectEquals::new("Priority", "Done")),
        page_size: PageSize::new(2).unwrap(),
    }
}

#[tokio::test]
async fn query_posts_exact_body_bytes_and_retrieval_has_no_body() {
    let script = Script::new([
        json_reply(200, QUERY1),
        json_reply(200, QUERY2),
        json_reply(200, PAGE),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let first = api
        .query_data_source("source/segment", &query(), None)
        .await
        .unwrap();
    api.query_data_source("source/segment", &query(), first.value.next_cursor)
        .await
        .unwrap();
    let page = api.retrieve_page("page/segment").await.unwrap();
    assert_eq!(page.meta.status(), reqwest::StatusCode::OK);
    assert_eq!(page.meta.headers()["x-request-id"], "fixture-request-id");
    assert_eq!(page.meta.operation().unwrap().method(), Some("GET"));
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer synthetic-notion-token")
        );
        assert_eq!(request.header("notion-version"), Some(NOTION_VERSION));
        assert_eq!(request.header("accept"), Some("application/json"));
    }
    for (request, expected) in requests[..2].iter().zip([
        include_bytes!("../fixtures/query-request-1.json").as_slice(),
        include_bytes!("../fixtures/query-request-2.json").as_slice(),
    ]) {
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v1/data_sources/source%2Fsegment/query");
        assert_eq!(request.header("content-type"), Some("application/json"));
        assert_eq!(request.body, expected);
    }
    assert_eq!(requests[2].method, "GET");
    assert_eq!(requests[2].target, "/v1/pages/page%2Fsegment");
    assert_eq!(requests[2].body, b"");
    assert_eq!(requests[2].header("content-type"), None);
    server.shutdown().await;
}

#[tokio::test]
async fn children_cursor_is_query_encoded_and_never_a_post_body() {
    let server = TestServer::start(|_| json_reply(200, LAST)).await;
    client(&server)
        .list_block_children(
            "parent/segment",
            PageSize::new(2).unwrap(),
            Some("cur/sor+ two".into()),
        )
        .await
        .unwrap();
    let request = server.single();
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.target,
        "/v1/blocks/parent%2Fsegment/children?page_size=2&start_cursor=cur%2Fsor%2B+two"
    );
    assert_eq!(request.body, b"");
    assert_eq!(request.header("notion-version"), Some(NOTION_VERSION));
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-notion-token")
    );
    assert_eq!(request.header("content-type"), None);
    server.shutdown().await;
}

#[tokio::test]
async fn concurrent_clients_keep_version_and_auth_context_independent() {
    let server = TestServer::start(|_| json_reply(200, PAGE)).await;
    let original = client(&server);
    let older = original.clone().with_version("2025-09-03");
    let other = NotionClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "synthetic-other-token",
    )
    .with_base_url(&format!("{}/v1", server.url()))
    .unwrap();
    let (a, b, c) = tokio::join!(
        older.retrieve_page("older"),
        original.retrieve_page("pinned"),
        other.retrieve_page("other")
    );
    a.unwrap();
    b.unwrap();
    c.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in requests {
        let (version, token) = match request.path() {
            "/v1/pages/older" => ("2025-09-03", "Bearer synthetic-notion-token"),
            "/v1/pages/pinned" => (NOTION_VERSION, "Bearer synthetic-notion-token"),
            "/v1/pages/other" => (NOTION_VERSION, "Bearer synthetic-other-token"),
            _ => panic!("unexpected request"),
        };
        assert_eq!(request.header_all("notion-version"), vec![version]);
        assert_eq!(request.header_all("authorization"), vec![token]);
    }
    assert!(!format!("{original:?}").contains("synthetic-notion-token"));
    server.shutdown().await;
}

#[tokio::test]
async fn header_injection_is_a_build_error_before_io() {
    let server = TestServer::start(|_| panic!("invalid header must not send")).await;
    let error = client(&server)
        .with_version("bad\r\nx-injected: yes")
        .retrieve_page("page")
        .await
        .unwrap_err();
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Build);
    assert!(error.meta().is_none());
    assert!(!format!("{error:?}").contains("x-injected"));
    assert!(server.requests().is_empty());
    assert!(PageSize::new(0).is_err());
    assert!(PageSize::new(101).is_err());
    server.shutdown().await;
}

#[tokio::test]
async fn documented_provider_failures_retain_code_status_and_headers() {
    let script = Script::new([
        json_reply(404, include_bytes!("../fixtures/error-404.json")),
        json_reply(429, include_bytes!("../fixtures/error-429.json")).header("Retry-After", "3"),
        json_reply(503, include_bytes!("../fixtures/error-503.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let missing = api.retrieve_page("missing").await.unwrap_err();
    let limited = api
        .list_block_children("limited", PageSize::default(), None)
        .await
        .unwrap_err();
    let unavailable = api
        .query_data_source("unavailable", &query(), None)
        .await
        .unwrap_err();
    for (failure, status, code) in [
        (&missing, 404, ErrorCode::ObjectNotFound),
        (&limited, 429, ErrorCode::RateLimited),
        (&unavailable, 503, ErrorCode::ServiceUnavailable),
    ] {
        let Error::Provider { error, meta } = failure else {
            panic!("expected provider error: {failure}")
        };
        assert_eq!(error.code, code);
        assert_eq!(meta.status().as_u16(), status);
        assert_eq!(meta.headers()["x-request-id"], "fixture-request-id");
        assert!(meta.operation().unwrap().is_endpoint());
        assert!(!format!("{failure:?} {failure}").contains("Synthetic fixture"));
    }
    assert_eq!(limited.meta().unwrap().headers()["retry-after"], "3");
    if let Error::Provider { error, .. } = missing {
        assert_eq!(
            error.request_id.as_deref(),
            Some("fixture-body-request-404")
        );
    }
    if let Error::Provider { error, .. } = unavailable {
        assert_eq!(
            error.additional_data.unwrap()["retry_guidance"][0],
            "Reduce page_size"
        );
    }
    assert_eq!(server.requests().len(), 3); // No retries.
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_and_unfamiliar_error_documents_remain_diagnostic() {
    let future = br#"{"object":"error","status":400,"code":"future_error","message":"private fixture detail"}"#;
    let mismatch = br#"{"object":"error","status":401,"code":"unauthorized","message":"private fixture detail"}"#;
    let script = Script::new([
        Reply::new(502)
            .header("Content-Type", "text/html")
            .body(b"<html>private fixture detail</html>"),
        json_reply(400, mismatch),
        json_reply(400, b"{invalid json"),
        json_reply(400, future),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for status in [502, 400, 400] {
        let failure = api.retrieve_page("page").await.unwrap_err();
        assert_eq!(failure.meta().unwrap().status().as_u16(), status);
        assert!(!format!("{failure:?}").contains("private fixture detail"));
        let Error::InvalidErrorBody { body, .. } = failure else {
            panic!("expected invalid error body")
        };
        assert!(!body.is_empty());
    }
    let failure = api.retrieve_page("page").await.unwrap_err();
    let Error::Provider { error, .. } = failure else {
        panic!("expected typed future error")
    };
    assert_eq!(error.code, ErrorCode::Other("future_error".into()));
    server.shutdown().await;
}

#[tokio::test]
async fn supported_properties_and_unknown_payloads_survive_together() {
    let script = Script::new([json_reply(200, PAGE), json_reply(200, CHILDREN2)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let page = api.retrieve_page("page").await.unwrap().value;
    let properties = &page.properties;
    for (name, expected) in [
        ("Name", "Plan café 🚀"),
        ("Estimate", "42.5"),
        ("Done", "true"),
        ("Priority", "Done"),
        ("Due", "2026-11-06..2026-11-07"),
        ("Notes", "Read $E=mc^2$[unsupported rich text: mention]"),
        ("Future", "[unsupported property: future_metric]"),
    ] {
        assert_eq!(render::property(&properties[name]), expected);
    }
    let raw: Value = serde_json::from_slice(PAGE).unwrap();
    let PropertyValue::Unknown { kind, raw: unknown } = &properties["Future"].value else {
        panic!("future property lost")
    };
    assert_eq!(kind, "future_metric");
    assert_eq!(*unknown, raw["properties"]["Future"]);
    let PropertyValue::Unknown { raw: relation, .. } = &properties["Related"].value else {
        panic!("relation not retained")
    };
    assert_eq!(relation["has_more"], true);
    assert_eq!(properties["Related"].id, "rel%3A1");
    let PropertyValue::RichText(fragments) = &properties["Notes"].value else {
        panic!("rich text lost")
    };
    let RichText::Unknown { raw: mention, .. } = &fragments[2] else {
        panic!("mention lost")
    };
    assert_eq!(*mention, raw["properties"]["Notes"]["rich_text"][2]);
    let children = api
        .list_block_children("page", PageSize::default(), None)
        .await
        .unwrap()
        .value;
    let future = &children.results[0];
    assert!(future.has_children);
    let BlockValue::Unknown { raw: unknown, .. } = &future.value else {
        panic!("future block lost")
    };
    let raw: Value = serde_json::from_slice(CHILDREN2).unwrap();
    assert_eq!(*unknown, raw["results"][0]);
    assert_eq!(render::block(future), "[unsupported block: future_callout]");
    assert_eq!(render::block(&children.results[1]), "- [x] Ship");
    server.shutdown().await;
}

#[tokio::test]
async fn supported_null_values_are_not_unknown_variants() {
    let mut page: Value = serde_json::from_slice(PAGE).unwrap();
    for (name, kind) in [
        ("Estimate", "number"),
        ("Priority", "select"),
        ("Due", "date"),
    ] {
        page["properties"][name][kind] = Value::Null;
    }
    let bytes = serde_json::to_vec(&page).unwrap();
    let server = TestServer::start(move |_| json_reply(200, &bytes)).await;
    let page = client(&server).retrieve_page("page").await.unwrap().value;
    assert!(matches!(
        page.properties["Estimate"].value,
        PropertyValue::Number(None)
    ));
    assert!(matches!(
        page.properties["Priority"].value,
        PropertyValue::Select(None)
    ));
    assert!(matches!(
        page.properties["Due"].value,
        PropertyValue::Date(None)
    ));
    for name in ["Estimate", "Priority", "Due"] {
        assert_eq!(render::property(&page.properties[name]), "");
    }
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_supported_variants_fail_decoding_instead_of_becoming_unknown() {
    let mut page: Value = serde_json::from_slice(PAGE).unwrap();
    page["properties"]["Estimate"]["number"] = json!("not a number");
    let mut rich_text: Value = serde_json::from_slice(PAGE).unwrap();
    rich_text["properties"]["Name"]["title"][0]["text"] = json!({"content": false});
    let mut blocks: Value = serde_json::from_slice(CHILDREN1).unwrap();
    blocks["results"][0]["heading_2"] = json!({"rich_text": null});
    let script = Script::new(
        [page, rich_text, blocks]
            .iter()
            .map(|v| json_reply(200, &serde_json::to_vec(v).unwrap())),
    );
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for failure in [
        api.retrieve_page("page").await.unwrap_err(),
        api.retrieve_page("page").await.unwrap_err(),
        api.list_block_children("page", PageSize::default(), None)
            .await
            .unwrap_err(),
    ] {
        assert_eq!(failure.concord().unwrap().kind(), ErrorKind::Decode);
        assert_eq!(failure.meta().unwrap().status().as_u16(), 200);
        assert!(failure.meta().unwrap().operation().is_some());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn wrong_success_media_type_and_truncated_body_keep_metadata() {
    let mut truncated = json_reply(200, b"");
    truncated.body = Body::FailAfter(vec![Bytes::from_static(b"{\"object\":")]);
    let script = Script::new([
        Reply::new(200)
            .header("Content-Type", "text/html")
            .body(PAGE),
        Reply::new(200).body(PAGE),
        truncated,
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for expected in [
        ErrorKind::Metadata,
        ErrorKind::Metadata,
        ErrorKind::Transport,
    ] {
        let failure = api.retrieve_page("page").await.unwrap_err();
        assert_eq!(failure.concord().unwrap().kind(), expected);
        assert_eq!(failure.meta().unwrap().status().as_u16(), 200);
        assert_eq!(
            failure.meta().unwrap().operation().unwrap().method(),
            Some("GET")
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn response_and_error_body_limits_are_bounded_and_keep_actual_status() {
    let success = vec![b' '; concord_sdk_notion::RESPONSE_LIMIT + 1];
    let error = vec![b' '; concord_sdk_notion::ERROR_LIMIT + 1];
    let script = Script::new([json_reply(200, &success), json_reply(503, &error)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    for status in [200, 503] {
        let failure = api.retrieve_page("page").await.unwrap_err();
        assert_eq!(failure.concord().unwrap().kind(), ErrorKind::Limit);
        assert_eq!(failure.meta().unwrap().status().as_u16(), status);
        assert!(failure.meta().unwrap().operation().is_some());
    }
    server.shutdown().await;
}
