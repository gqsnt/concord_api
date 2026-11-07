//! Secret: the bearer token (`NotionClient::new(http, token)`, `src/client.rs:51`; `#[sensitive]
//! token`, `src/api.rs:13`, sent as `Authorization: Bearer`). One canary. The server echoes it
//! into headers, the provider error `message` and `request_id` (withheld: `Error`'s `Debug` and
//! `Display` omit the document), the success payloads (`Response`'s `Debug` omits the value) and
//! every malformed, wrong-media and truncated body. Read-only SDK: `retrieve_page`, the
//! `query_data_source` POST and `list_block_children` cover the seven scenarios through the SDK
//! and the generated client's native `Prepared`; a `children` traversal is formatted beside
//! them. Client, generated client and config, endpoints, `Prepared`, responses, metadata,
//! traversal, `PagesError`, `Error` and its source are formatted.
//! Not covered: native reqwest values; clear decoded models and documents (`Page`, `List`,
//! `ProviderError` fields, `InvalidErrorBody::body`: documented unredacted, so not formatted);
//! cursors (not declared sensitive). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

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
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

fn problem(status: u16, code: &str, echo: &str) -> String {
    format!(
        r#"{{"object":"error","status":{status},"code":"{code}","message":"{echo}","request_id":"{echo}"}}"#
    )
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem(401, "unauthorized", echo)),
        2 => Reply::json(500, problem(500, "internal_server_error", echo)),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord_sdk_notion::api::{Notion, NotionConfig};
use concord_sdk_notion::{Error, NotionClient, PageSize, Query, Response, SelectEquals};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    if let Some(e) = error.concord() {
        concord_error(c, e);
    }
}

fn result<T>(c: &mut Canaries, label: &str, result: &Result<Response<T>, Error>) {
    c.assert_debug_absent(label, result);
    match result {
        Ok(response) => c.assert_debug_absent("response metadata", &response.meta),
        Err(e) => error(c, e),
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("notion");
    let token = c.issue();
    let query = Query {
        filter: Some(SelectEquals::new("Priority", "Done")),
        page_size: PageSize::new(10).unwrap(),
    };
    let size = PageSize::new(10).unwrap();
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.path().starts_with("/v1/pages/") {
                include_str!("../fixtures/page.json").replace("Plan café 🚀", &echo)
            } else {
                r#"{"object":"list","results":[],"next_cursor":null,"has_more":false,"request_status":{"type":"complete"}}"#.into()
            };
            reply(case, &success, &echo)
        })
        .await;
        let base = format!("{}/v1", if case == 6 { refused() } else { server.url() });
        let config = NotionConfig {
            token: token.clone(),
        };
        c.assert_debug_absent("config", &config);
        let generated = Notion::new(http(), config).with_base_url(&base).unwrap();
        c.assert_debug_absent("generated client", &generated);
        let notion = NotionClient::new(http(), &token)
            .with_base_url(&base)
            .unwrap();
        c.assert_debug_absent("client", &notion);
        let page = notion.api().page("page-1");
        let children = notion.api().children("block-1", 10);
        c.assert_debug_absent("page endpoint", &page);
        c.assert_debug_absent("children endpoint", &children);
        c.assert_debug_absent("children Prepared", &children.prepare().unwrap());
        let prepared = page.prepare().unwrap();
        c.assert_debug_absent("page Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let read = notion.retrieve_page("page-1").await;
        result(&mut c, "page Result", &read);
        assert_eq!(read.is_ok(), case == 0);
        let listed = notion.query_data_source("source-1", &query, None).await;
        result(&mut c, "query Result", &listed);
        assert_eq!(listed.is_ok(), case == 0);
        let blocks = notion.list_block_children("block-1", size, None).await;
        result(&mut c, "children Result", &blocks);
        let mut pages = notion.children("block-1", size);
        c.assert_debug_absent("traversal", &pages);
        match pages.next_page().await {
            Ok(page) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("traversal page", &page);
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "notion: {} renderings; positive control passed",
        c.checked()
    );
}
