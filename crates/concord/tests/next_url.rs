//! `page next link;` / `page next url a?.b;` on a client without a send policy: a followed page is
//! the same endpoint sent once to the provider's URL with the declared headers, decoding and limit;
//! another origin is refused before any I/O; the URL never appears in Concord's output.

mod support;

use concord::reqwest::header::AUTHORIZATION;
use concord::{Endpoint, ErrorKind, Follow, Json, NextUrl, NextUrlError, UrlContinuation, reqwest};
use serde::Deserialize;
use support::{Reply, Server};

const CANARY: &str = "F3_POLICYLESS_CURSOR_5521";

/// One page of items with the provider's next URL.
#[derive(Debug, Deserialize)]
pub struct ItemPage {
    /// Records.
    pub items: Vec<u64>,
    /// Pagination links, absent on the last page.
    pub links: Option<Links>,
}

/// The provider's links.
#[derive(Debug, Deserialize)]
pub struct Links {
    /// The next page's URL; null on the last page.
    pub next: Option<String>,
}

concord::api! {
    pub client Plain(#[sensitive] header(AUTHORIZATION) token: String) {
        base "http://127.0.0.1";
        format Json;
        GET items(query per_page: u32 = 2) -> ItemPage {
            path "items";
            page next url links?.next;
        }
        GET small() -> ItemPage {
            path "small";
            limit 32;
            page next link;
        }
    }
}

fn client(server: &Server) -> Plain {
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    Plain::new(
        http,
        PlainConfig {
            token: "Bearer SYNTHETIC_TOKEN".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

#[tokio::test]
async fn a_followed_page_is_the_same_endpoint_sent_once_with_its_default_send() {
    let server = Server::start(|request| match request.target.as_str() {
        "/items?per_page=2" => Reply::json(
            200,
            &format!(
                r#"{{"items":[1,2],"links":{{"next":"http://{}/items?b=2&a=1&cursor={CANARY}"}}}}"#,
                request.header("host").unwrap()
            ),
        ),
        _ => Reply::json(200, r#"{"items":[3],"links":{"next":null}}"#),
    })
    .await;
    let api = client(&server);
    let endpoint = api.items();
    let first: ItemPage = endpoint.clone().send().await.unwrap();
    assert_eq!(first.items, [1, 2]);
    let next = PlainItems::next_url(&first).unwrap().unwrap();
    let followed = endpoint.follow(next);
    assert!(PlainItems::operation().is_read());
    assert!(<concord::Followed<PlainItems> as Endpoint<ItemPage>>::operation().is_read());
    assert!(
        followed
            .clone()
            .prepare()
            .unwrap()
            .operation()
            .unwrap()
            .is_read()
    );
    let shown = format!(
        "{followed:?} {:?}",
        concord::Endpoint::prepare(followed.clone()).unwrap()
    );
    assert!(!shown.contains(CANARY), "{shown}");
    assert_eq!(
        server.requests().len(),
        1,
        "follow and prepare send nothing"
    );

    // The policy-less default: `Result<ItemPage, concord::Error>`, exactly as `send()`.
    let second: Result<ItemPage, concord::Error> = followed.send().await;
    let second = second.unwrap();
    assert_eq!(second.items, [3]);
    assert!(PlainItems::next_url(&second).unwrap().is_none());

    let wire = server.requests();
    assert_eq!(wire.len(), 2, "one request per page");
    assert_eq!(wire[1].target, format!("/items?b=2&a=1&cursor={CANARY}"));
    assert_eq!(wire[1].method, "GET");
    assert_eq!(
        wire[1].header("authorization"),
        Some("Bearer SYNTHETIC_TOKEN")
    );
}

#[tokio::test]
async fn a_followed_page_keeps_the_declared_limit_and_identity() {
    let server = Server::start(|request| match request.target.as_str() {
        "/small" => Reply::json(200, r#"{"items":[]}"#),
        _ => Reply::json(200, r#"{"items":[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15]}"#),
    })
    .await;
    let api = client(&server);
    let next = NextUrl::parse(&format!("{}/small?cursor={CANARY}", server.url())).unwrap();
    let error = api.small().follow(next).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    assert!(error.operation().unwrap().is_read());
    assert_eq!(
        error.operation().map(|o| o.name()),
        Some(PlainSmall::operation().name())
    );
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains(CANARY), "{shown}");
}

#[tokio::test]
async fn a_url_on_another_origin_is_refused_before_any_request() {
    let server = Server::reply(Reply::json(200, r#"{"items":[]}"#)).await;
    let foreign = Server::reply(Reply::json(200, r#"{"items":[]}"#)).await;
    let api = client(&server);
    let next = NextUrl::parse(&format!("{}/items?cursor={CANARY}", foreign.url())).unwrap();
    let error = api.items().follow(next).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let shown = format!("{error} {error:?}");
    assert!(shown.contains("another origin"), "{shown}");
    assert!(!shown.contains(CANARY), "{shown}");
    assert!(
        foreign.requests().is_empty(),
        "the foreign origin received a request"
    );
    assert!(server.requests().is_empty());
}

#[test]
fn a_present_next_url_must_be_absolute_and_absent_or_null_ends() {
    let page = |next: Option<&str>| ItemPage {
        items: Vec::new(),
        links: Some(Links {
            next: next.map(str::to_owned),
        }),
    };
    assert_eq!(
        PlainItems::next_url(&page(Some(""))),
        Err(NextUrlError::Empty)
    );
    assert_eq!(
        PlainItems::next_url(&page(Some("/items?page=2"))),
        Err(NextUrlError::Relative)
    );
    assert_eq!(
        PlainItems::next_url(&page(Some("http://[::1"))),
        Err(NextUrlError::Malformed)
    );
    assert_eq!(PlainItems::next_url(&page(None)), Ok(None));
    let absent = ItemPage {
        items: Vec::new(),
        links: None,
    };
    assert_eq!(PlainItems::next_url(&absent), Ok(None));
    let url = format!("http://127.0.0.1/items?cursor={CANARY}");
    let next = PlainItems::next_url(&page(Some(&url))).unwrap().unwrap();
    assert_eq!(next.expose(), url);
    assert!(!format!("{next} {next:?}").contains(CANARY));
}
