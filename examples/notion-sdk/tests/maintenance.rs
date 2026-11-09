//! Fresh additive-schema fixtures, with overlapping requests on both children pages.
use concord::ErrorKind;
use concord_example_testserver::{Gate, Recorded, Reply, Script, TestServer};
use concord_sdk_notion::models::{Block, BlockValue, Page, PropertyValue, RichText};
use concord_sdk_notion::pagination::{PagesError, Resume};
use concord_sdk_notion::{
    Error, ErrorCode, NOTION_VERSION, NotionClient, PageSize, render, reqwest,
};
use serde_json::{Value, json};
use std::{
    num::NonZeroUsize,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

const PAGE: &[u8] = include_bytes!("../fixtures/maintenance-page.json");
const FIRST: &[u8] = include_bytes!("../fixtures/maintenance-children-1.json");
const SECOND: &[u8] = include_bytes!("../fixtures/maintenance-children-2.json");
const CURSOR: &str = " opaque/%2F+ two?x=1&y=雪 \t";
const CURSOR_QUERY: &str = "start_cursor=+opaque%2F%252F%2B+two%3Fx%3D1%26y%3D%E9%9B%AA+%09";
const TOKEN: &str = "Bearer synthetic-maintenance-token";
const OLDER_VERSION: &str = "2025-09-03";
const RESPONSE_VERSION: &str = "synthetic-response-version";

fn reply(body: &[u8]) -> Reply {
    Reply::new(200)
        .header("Content-Type", "application/json")
        .header("Notion-Version", RESPONSE_VERSION)
        .body(body)
}

fn client(server: &TestServer) -> NotionClient {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    NotionClient::new(http, "synthetic-maintenance-token")
        .with_base_url(&format!("{}/v1", server.url()))
        .unwrap()
}

fn size() -> PageSize {
    PageSize::new(5).unwrap()
}

fn check_children_request(request: &Recorded, parent: &str, cursor: bool) {
    assert_eq!(request.method, "GET");
    let mut expected = format!("/v1/blocks/{parent}/children?page_size=5");
    if cursor {
        expected.push('&');
        expected.push_str(CURSOR_QUERY);
    }
    assert_eq!(request.target, expected);
    assert_eq!(request.query_all("page_size"), ["5"]);
    assert_eq!(
        request.query_all("start_cursor"),
        if cursor { vec![CURSOR] } else { vec![] }
    );
    assert!(request.body.is_empty());
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.header_all("authorization"), [TOKEN]);
}

fn check_properties(page: &Page) {
    let raw: Value = serde_json::from_slice(PAGE).unwrap();
    assert_eq!(page.url.as_deref().unwrap(), raw["url"].as_str().unwrap());
    let p = &page.properties;
    assert_eq!(p.len(), 7);
    assert!(matches!(p["Name"].value, PropertyValue::Title(_)));
    assert!(matches!(
        p["Estimate"].value,
        PropertyValue::Number(Some(3.5))
    ));
    assert!(matches!(p["Done"].value, PropertyValue::Checkbox(false)));
    assert!(matches!(
        p["Priority"].value,
        PropertyValue::Select(Some(_))
    ));
    assert!(matches!(p["Due"].value, PropertyValue::Date(Some(_))));
    let PropertyValue::RichText(text) = &p["Notes"].value else {
        panic!("supported rich text lost")
    };
    let RichText::Text(first) = &text[0] else {
        panic!("supported text lost")
    };
    assert_eq!(
        first.link.as_ref().unwrap().url,
        raw["properties"]["Notes"]["rich_text"][0]["text"]["link"]["url"]
            .as_str()
            .unwrap()
    );
    assert!(matches!(text[1], RichText::Equation(_)));
    let PropertyValue::Unknown { kind, raw: unknown } = &p["Progress"].value else {
        panic!("new property type was not retained")
    };
    assert_eq!(kind, "future_progress");
    assert_eq!(*unknown, raw["properties"]["Progress"]);
    assert_eq!(p["Progress"].id, "progress%3A%2f+");
    for (name, expected) in [
        ("Name", "Maintenance café"),
        ("Notes", "Guide$x+1$"),
        ("Estimate", "3.5"),
        ("Done", "false"),
        ("Priority", "Ready"),
        ("Due", "2026-11-06"),
        ("Progress", "[unsupported property: future_progress]"),
    ] {
        assert_eq!(render::property(&p[name]), expected);
    }
}

fn check_blocks(blocks: &[&Block]) {
    assert_eq!(blocks.len(), 8);
    assert!(matches!(blocks[0].value, BlockValue::Heading1(_)));
    assert!(matches!(blocks[1].value, BlockValue::Paragraph(_)));
    assert!(matches!(blocks[2].value, BlockValue::BulletedListItem(_)));
    assert!(matches!(blocks[4].value, BlockValue::Heading2(_)));
    assert!(matches!(blocks[5].value, BlockValue::Heading3(_)));
    assert!(matches!(blocks[6].value, BlockValue::ToDo(_)));
    assert!(matches!(blocks[7].value, BlockValue::Divider));
    let unknown = blocks[3];
    assert_eq!(unknown.id, "maintenance-unknown");
    assert!(unknown.has_children);
    let BlockValue::Unknown { kind, raw } = &unknown.value else {
        panic!("new block type was not retained")
    };
    assert_eq!(kind, "future_panel");
    let second: Value = serde_json::from_slice(SECOND).unwrap();
    assert_eq!(*raw, second["results"][0]);
    let BlockValue::Paragraph(paragraph) = &blocks[1].value else {
        unreachable!()
    };
    let RichText::Text(text) = &paragraph.rich_text[0] else {
        unreachable!()
    };
    let first: Value = serde_json::from_slice(FIRST).unwrap();
    assert_eq!(
        text.link.as_ref().unwrap().url,
        first["results"][1]["paragraph"]["rich_text"][0]["text"]["link"]["url"]
            .as_str()
            .unwrap()
    );
    assert_eq!(
        blocks
            .iter()
            .map(|block| render::block(block))
            .collect::<Vec<_>>(),
        [
            "# Maintenance",
            "Keep café",
            "- Review",
            "[unsupported block: future_panel]",
            "## Details",
            "### $x+1$",
            "- [ ] Ship",
            "---"
        ]
    );
}

#[tokio::test]
async fn new_variants_and_second_pages_keep_captured_versions_during_overlap() {
    let first_gate = Gate::new();
    let second_gate = Gate::new();
    let first_count = AtomicUsize::new(0);
    let second_count = AtomicUsize::new(0);
    let server = TestServer::start(move |request| {
        let (body, gate, count, expected) = match request.path() {
            "/v1/pages/pinned" | "/v1/pages/older" => (PAGE, &first_gate, &first_count, 4),
            "/v1/blocks/pinned/children" | "/v1/blocks/older/children" => {
                if request.query_all("start_cursor").is_empty() {
                    (FIRST, &first_gate, &first_count, 4)
                } else {
                    (SECOND, &second_gate, &second_count, 2)
                }
            }
            "/v1/blocks/maintenance-unknown/children" => {
                return reply(include_bytes!("../fixtures/unknown-children.json"));
            }
            _ => panic!("unexpected maintenance request"),
        };
        // No response is released until both clients have outstanding requests.
        let arrived = count.fetch_add(1, Ordering::SeqCst) + 1;
        assert!(arrived <= expected);
        if arrived == expected {
            gate.open();
        }
        reply(body).after(gate.clone())
    })
    .await;
    let original = client(&server);
    let pinned_client = original.clone();
    // Capture a lazy traversal before consuming the originating client in with_version.
    let pinned = original
        .children("pinned", size())
        .max_pages(NonZeroUsize::new(2).unwrap());
    let older_client = original.with_version(OLDER_VERSION);
    let older = older_client
        .children("older", size())
        .max_pages(NonZeroUsize::new(2).unwrap());
    let (a, b, page_a, page_b) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(
            pinned.collect_complete(),
            older.collect_complete(),
            pinned_client.retrieve_page("pinned"),
            older_client.retrieve_page("older"),
        )
    })
    .await
    .expect("overlapping requests must complete");
    for pages in [a.unwrap(), b.unwrap()] {
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].items.len(), 3); // Fewer than page_size does not end a list.
        assert_eq!(pages[0].next.as_deref(), Some(CURSOR));
        assert!(pages[1].next.is_none());
        for page in &pages {
            assert_eq!(page.meta.headers()["notion-version"], RESPONSE_VERSION);
            assert_eq!(page.meta.operation().unwrap().method(), Some("GET"));
        }
        check_blocks(
            &pages
                .iter()
                .flat_map(|page| &page.items)
                .collect::<Vec<_>>(),
        );
    }
    for page in [page_a.unwrap(), page_b.unwrap()] {
        check_properties(&page.value);
    }
    // An unknown parent remains expandable through its ordinary identity/has_children.
    let nested = pinned_client
        .children("maintenance-unknown", size())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(nested.len(), 1);
    assert_eq!(render::block(&nested[0]), "Inside future block.");
    let requests = server.requests();
    assert_eq!(requests.len(), 7); // No third page or hyperlink fetch.
    for request in &requests {
        let version = if request.path().contains("/older") {
            OLDER_VERSION
        } else {
            NOTION_VERSION
        };
        assert_eq!(request.header_all("notion-version"), [version]);
        assert_eq!(request.header_all("authorization"), [TOKEN]);
        assert_eq!(request.header("accept"), Some("application/json"));
        if request.path().starts_with("/v1/blocks/") {
            let parent = request.path().split('/').nth(3).unwrap();
            check_children_request(
                request,
                parent,
                !request.query_all("start_cursor").is_empty(),
            );
        } else {
            assert_eq!(request.method, "GET");
            assert!(request.query_pairs().is_empty());
            assert!(request.body.is_empty());
        }
    }
    for parent in ["pinned", "older"] {
        let path = format!("/v1/blocks/{parent}/children");
        let children: Vec<_> = requests.iter().filter(|r| r.path() == path).collect();
        assert_eq!(children.len(), 2);
        check_children_request(children[0], parent, false);
        check_children_request(children[1], parent, true);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn cap_retains_exact_cursor_and_resume_includes_the_new_unknown_block() {
    let script = Script::new([reply(FIRST), reply(SECOND)]);
    let server = TestServer::start(move |_| script.next()).await;
    let api = client(&server);
    let mut pages = api
        .children("pinned", size())
        .max_pages(NonZeroUsize::new(1).unwrap());
    let first = pages.next_page().await.unwrap().unwrap();
    let bounded = pages.next_page().await.unwrap_err();
    assert!(
        matches!(&bounded, PagesError::Bounded { resume: Resume::At(cursor), .. } if cursor == CURSOR)
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended { .. })
    ));
    assert_eq!(server.requests().len(), 1);
    let mut resumed = api
        .children("pinned", size())
        .starting_at(bounded.resume().unwrap().clone())
        .max_pages(NonZeroUsize::new(1).unwrap());
    let second = resumed.next_page().await.unwrap().unwrap();
    assert!(resumed.next_page().await.unwrap().is_none());
    check_blocks(&first.items.iter().chain(&second.items).collect::<Vec<_>>());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_children_request(&requests[0], "pinned", false);
    check_children_request(&requests[1], "pinned", true);
    server.shutdown().await;
}

#[tokio::test]
async fn failed_second_page_preserves_prefix_and_typed_error_with_opaque_cursor() {
    let script = Script::new([
        reply(FIRST),
        Reply::new(429)
            .header("Content-Type", "application/json")
            .header("Retry-After", "7")
            .body(include_bytes!("../fixtures/error-429.json")),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let partial = client(&server)
        .children("pinned", size())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 3);
    let PagesError::Provider {
        error: Error::Provider { error, meta },
        failed,
        ..
    } = partial.error
    else {
        panic!("expected typed provider failure")
    };
    assert_eq!(failed, Resume::At(CURSOR.into()));
    assert_eq!(error.code, ErrorCode::RateLimited);
    assert_eq!(meta.status().as_u16(), 429);
    assert_eq!(meta.headers()["retry-after"], "7");
    assert_eq!(meta.operation().unwrap().method(), Some("GET"));
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_children_request(&requests[1], "pinned", true);
    server.shutdown().await;
}

#[tokio::test]
async fn a_new_unknown_does_not_hide_a_malformed_supported_type_on_page_two() {
    let mut malformed: Value = serde_json::from_slice(SECOND).unwrap();
    malformed["results"][3]["to_do"]["checked"] = json!("not a boolean");
    let script = Script::new([
        reply(FIRST),
        reply(&serde_json::to_vec(&malformed).unwrap()),
    ]);
    let server = TestServer::start(move |_| script.next()).await;
    let partial = client(&server)
        .children("pinned", size())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 3); // No part of the malformed page is emitted.
    let PagesError::Provider { error, failed, .. } = partial.error else {
        panic!("expected decoding failure")
    };
    assert_eq!(failed, Resume::At(CURSOR.into()));
    assert_eq!(error.concord().unwrap().kind(), ErrorKind::Decode);
    assert_eq!(error.meta().unwrap().status().as_u16(), 200);
    assert_eq!(
        error.meta().unwrap().operation().unwrap().method(),
        Some("GET")
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    check_children_request(&requests[1], "pinned", true);
    server.shutdown().await;
}
