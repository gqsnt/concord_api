//! Pages against a real server (C8.4, C8.6): a server that pages with `Link` headers, absolute
//! and relative; one that puts the next URL in the body, an empty query included; and one
//! that reports its page numbers. Each walk ends where the server says, and asks for no page
//! more.

use crate::server::{Server, Wire};
use axum::extract::Request;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use concord::{by_link, by_next_url, by_page_bounds, Json};
use serde::Deserialize;

/// A page whose next URL is in the body.
#[derive(Debug, Deserialize)]
pub struct Linked {
    items: Vec<u32>,
    next: Option<String>,
}

/// A page that reports its number and the number of the last page.
#[derive(Debug, Deserialize)]
pub struct Numbered {
    rows: Vec<u32>,
    page: u32,
    pages: u32,
}

concord::api! {
    client Catalog {
        base "http://catalog.test";
        read Json;
        GET linked() -> Vec<u32> { path "linked"; paginate by_link(); }
        GET in_body() -> Linked {
            path "slice";
            paginate by_next_url(|page: &Linked| page.next.clone());
        }
        GET numbered(page?: u32) -> Numbered {
            path "numbered";
            query { page; }
            paginate by_page_bounds(page, |list| Some((list.page, list.pages)));
        }
        GET from_zero(page?: u32) -> Numbered {
            path "from-zero";
            query { page; }
            paginate by_page_bounds(page, |list| Some((list.page, list.pages)));
        }
    }
}

/// The number in `page=N` of the query, or 1.
fn page_of(request: &Request) -> u32 {
    request
        .uri()
        .query()
        .and_then(|query| query.strip_prefix("page="))
        .and_then(|n| n.parse().ok())
        .unwrap_or(1)
}

/// `http://<host>` of the request, as the client wrote it (`Host`, or `:authority`).
fn origin_of(request: &Request) -> String {
    let host = request
        .uri()
        .authority()
        .map(|authority| authority.as_str().to_owned())
        .or_else(|| {
            request
                .headers()
                .get("host")
                .and_then(|host| host.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap();
    format!("http://{host}")
}

fn json(text: String) -> Response {
    ([("content-type", "application/json")], text).into_response()
}

fn catalog() -> Router {
    Router::new()
        .route(
            "/linked",
            get(|request: Request| async move {
                let page = page_of(&request);
                let link = match page {
                    // An absolute URL, at the origin of the call.
                    1 => format!("<{}/linked?page=2>; rel=\"next\"", origin_of(&request)),
                    // A relative URL, and another relation that is not followed.
                    2 => {
                        "</linked?page=1>; rel=\"prev\", </linked?page=3>; rel=\"next\"".to_owned()
                    }
                    // The last page has no next link.
                    _ => "</linked?page=2>; rel=\"prev\"".to_owned(),
                };
                let mut response = json(format!("[{}, {}]", page * 10, page * 10 + 1));
                response.headers_mut().insert("link", link.parse().unwrap());
                response
            }),
        )
        .route(
            "/slice",
            get(|request: Request| async move {
                // `/slice` and `/slice?` are two requests: the second has an empty query.
                if request.uri().query().is_none() {
                    json(r#"{"items": [1, 2], "next": "/slice?"}"#.to_owned())
                } else {
                    json(r#"{"items": [3], "next": null}"#.to_owned())
                }
            }),
        )
        .route(
            "/numbered",
            get(|request: Request| async move {
                let page = page_of(&request);
                json(format!(
                    r#"{{"rows": [{}], "page": {page}, "pages": 3}}"#,
                    page * 100
                ))
            }),
        )
        .route(
            "/from-zero",
            get(|request: Request| async move {
                // Pages 0, 1 and 2: the last page is number 2. A request with no page gets
                // page 0, and the server says so.
                let page = match request.uri().query() {
                    None => 0,
                    Some(_) => page_of(&request),
                };
                json(format!(
                    r#"{{"rows": [{page}], "page": {page}, "pages": 2}}"#
                ))
            }),
        )
}

async fn walks(wire: Wire) {
    let server = Server::start(catalog()).await;
    let catalog = Catalog::new()
        .with_transport(wire.http())
        .with_base(server.url());

    let mut pages = catalog.linked().pages();
    let mut items = Vec::new();
    while let Some(page) = pages.try_next().await.unwrap() {
        items.extend(page);
    }
    assert_eq!(items, [10, 11, 20, 21, 30, 31]);
    assert_eq!(
        server.seen.targets(),
        ["/linked", "/linked?page=2", "/linked?page=3"]
    );

    // A next URL is sent as the server gave it, an empty query included (G10, C8.6).
    let mut pages = catalog.in_body().pages();
    let mut items = Vec::new();
    while let Some(page) = pages.try_next().await.unwrap() {
        items.extend(page.items);
    }
    assert_eq!(items, [1, 2, 3]);
    assert_eq!(server.seen.targets()[3..], ["/slice", "/slice?"]);

    // The bounds that the server reports: pages 1, 2 and 3 of 3, then the end.
    let mut pages = catalog.numbered().pages();
    let mut rows = Vec::new();
    while let Some(page) = pages.try_next().await.unwrap() {
        rows.extend(page.rows);
    }
    assert_eq!(rows, [100, 200, 300]);
    assert_eq!(
        server.seen.targets()[5..8],
        ["/numbered", "/numbered?page=2", "/numbered?page=3"]
    );

    // A server whose pages count from zero: the next page is the reported one plus one.
    let mut pages = catalog.from_zero().pages();
    let mut rows = Vec::new();
    while let Some(page) = pages.try_next().await.unwrap() {
        rows.extend(page.rows);
    }
    assert_eq!(rows, [0, 1, 2]);
    assert_eq!(
        server.seen.targets()[8..],
        ["/from-zero", "/from-zero?page=1", "/from-zero?page=2"]
    );
}

#[tokio::test]
async fn c8_6_link_headers_next_urls_and_page_numbers_end_where_the_server_says_over_http1() {
    walks(Wire::One).await;
}

#[tokio::test]
async fn c8_6_link_headers_next_urls_and_page_numbers_end_where_the_server_says_over_http2() {
    walks(Wire::Two).await;
}
