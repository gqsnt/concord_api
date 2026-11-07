//! Secrets: the per-shop access token (`AccessToken::new`, `src/scalars.rs:161-187`; the shop
//! scope's `#[sensitive] header(X_SHOPIFY_ACCESS_TOKEN)`, `src/api.rs:40`), and the values its
//! types' `Debug` withholds: a connection cursor (`Cursor`, `src/scalars.rs:194`), search
//! terms (`SearchQuery`, `src/search.rs:87`; the products filter and the bulk export filter,
//! which `RunBulkQuery` withholds) and a signed bulk-result URL (`SignedUrl`,
//! `src/scalars.rs:218`). Each has its own canary. The token travels as
//! `X-Shopify-Access-Token`, cursor and search terms in the GraphQL body, and the signed URL
//! (returned by `bulkOperation`) as the target of `result_download`. The server echoes the
//! token into headers, GraphQL error and user-error messages (withheld), an `endCursor`, the
//! 4xx/5xx bodies and every malformed, wrong-media and truncated body. Read (`products`) and
//! write (`run_bulk_query`, a mutation) cover the seven scenarios through `ShopContext::send`
//! and the native `Prepared`; the success case also reads a bulk operation and downloads its
//! result. Contexts, generated handles, endpoints, `Prepared`, envelopes, data, connections,
//! cursors, bulk operations, `RequestError` and Concord errors are formatted.
//! Not covered: native reqwest values (`result_download`'s request), explicit `expose()`
//! accessors and clear model fields. `ProductsQuery` Debug withholding the search terms
//! `filter` copies from a `SearchQuery` is checked separately below. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::time::Duration;

fn http(server: &TestServer) -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .resolve("shop-one.myshopify.com", server.addr())
        .build()
        .unwrap()
}

// A refused connection: a port nothing listens on, still under the shop's subdomain.
fn refused_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, format!(r#"{{"errors":"{echo}"}}"#)),
        2 => Reply::json(500, format!(r#"{{"errors":"{echo}"}}"#)),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("x-shopify-api-version", "2026-10")
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

use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::bulk::BulkOperationQuery;
use concord_example_shopify_sdk::ops::catalog::ProductsQuery;
use concord_example_shopify_sdk::scalars::BulkOperationId;
use concord_example_shopify_sdk::{
    AccessToken, CatalogExport, Cursor, GraphqlResponse, RequestError, SearchQuery, ShopDomain,
    ShopifyContext,
};

fn answer<D: std::fmt::Debug>(
    c: &mut Canaries,
    label: &str,
    result: &Result<GraphqlResponse<D>, RequestError>,
) {
    c.assert_debug_absent(label, result);
    match result {
        Ok(envelope) => {
            c.assert_debug_absent("GraphQL errors", &envelope.errors);
            c.assert_debug_absent("cost", &envelope.cost());
            if let Some(data) = envelope.data.as_present() {
                c.assert_debug_absent("data", data);
            }
            for e in &envelope.errors {
                c.assert_display_absent("GraphQL error", e);
            }
        }
        Err(e) => {
            c.assert_debug_absent("RequestError", e);
            c.assert_error_absent("RequestError and sources", e);
            if let Some(e) = e.concord() {
                concord_error(c, e);
            }
        }
    }
}

fn products(search: &SearchQuery, cursor: &Cursor) -> ProductsQuery {
    let size = PageSize::new(10).unwrap();
    ProductsQuery::new(size, size)
        .after(Some(cursor.clone()))
        .filter(search)
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("shopify");
    let token = AccessToken::new(c.issue()).unwrap();
    let cursor = Cursor::new(c.issue());
    let search = SearchQuery::new().term("vendor", &c.issue());
    let export = CatalogExport::products().filter(SearchQuery::new().term("title", &c.issue()));
    let signature = c.issue();
    c.assert_debug_absent("access token", &token);
    c.assert_debug_absent("cursor", &cursor);
    c.assert_debug_absent("search query", &search);
    c.assert_debug_absent("catalog export", &export);
    c.assert_debug_absent("bulk mutation", &export.run());
    for case in 0..7 {
        let (echo, signature) = (token.expose().to_owned(), signature.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let body = r.body_text();
            let success = if r.path() == "/results.jsonl" {
                String::new()
            } else if body.contains("BulkRunQuery") {
                format!(
                    r#"{{"data":{{"bulkOperationRunQuery":{{"bulkOperation":null,"userErrors":[{{"field":["query"],"message":"{echo}","code":"INVALID"}}]}}}}}}"#
                )
            } else if body.contains("BulkOperationStatus") {
                format!(
                    r#"{{"data":{{"bulkOperation":{{"id":"gid://shopify/BulkOperation/1","status":"COMPLETED","errorCode":null,"createdAt":"2026-11-04T00:00:00Z","completedAt":"2026-11-04T00:01:00Z","objectCount":"0","rootObjectCount":"0","fileSize":null,"url":"http://{}/results.jsonl?signature={signature}","partialDataUrl":null,"type":"QUERY"}}}}}}"#,
                    r.host
                )
            } else {
                format!(
                    r#"{{"data":{{"products":{{"nodes":[],"pageInfo":{{"hasNextPage":true,"endCursor":"{echo}"}}}}}},"errors":[{{"message":"{echo}","path":["products"],"extensions":{{"code":"INTERNAL_SERVER_ERROR"}}}}],"extensions":{{"cost":{{"requestedQueryCost":1,"actualQueryCost":1,"throttleStatus":{{"maximumAvailable":2000.0,"currentlyAvailable":1999,"restoreRate":100.0}}}}}}}}"#
                )
            };
            reply(case, &success, &echo)
        })
        .await;
        let port = if case == 6 {
            refused_port()
        } else {
            server.addr().port()
        };
        let sdk = ShopifyContext::new(http(&server))
            .with_base_url(format!("http://myshopify.com:{port}"))
            .unwrap();
        let shop = sdk.shop(ShopDomain::new("shop-one").unwrap(), &token);
        let admin = shop.admin();
        let read = admin.products(products(&search, &cursor));
        let write = admin.run_bulk_query(export.run());
        c.assert_debug_absent("SDK context", &sdk);
        c.assert_debug_absent("generated client", sdk.admin());
        c.assert_debug_absent("shop context", &shop);
        c.assert_debug_absent("shop handle", admin);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("write Prepared", &write.clone().prepare());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let result = shop.send(read.prepare()).await;
        answer(&mut c, "read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if let Ok(envelope) = &result {
            let page = &envelope.data.as_present().unwrap().products.page_info;
            c.assert_debug_absent("page info", page);
            c.assert_debug_absent("next cursor", &page.next(Some(&cursor)));
        }
        let result = shop.send(write.prepare()).await;
        answer(&mut c, "write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        if case == 0 {
            let status = BulkOperationQuery {
                id: BulkOperationId::parse("gid://shopify/BulkOperation/1").unwrap(),
            };
            let result = shop.send(admin.bulk_operation(status).prepare()).await;
            answer(&mut c, "bulk status Result", &result);
            let operation = result.unwrap().data.present().unwrap().bulk_operation;
            let url = operation.as_ref().unwrap().url.clone().unwrap();
            c.assert_debug_absent("bulk operation", &operation);
            c.assert_debug_absent("signed URL", &url);
            let download = shop.http().execute(shop.result_download(&url)).await;
            assert!(download.is_ok());
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 250);
    println!(
        "shopify: {} renderings; positive control passed",
        c.checked()
    );
}

#[test]
fn products_query_debug_withholds_search_terms() {
    let mut c = Canaries::new("shopify");
    let search = SearchQuery::new().term("vendor", &c.issue());
    let cursor = Cursor::new("opaque");
    c.assert_debug_absent("SearchQuery", &search);
    c.assert_debug_absent("ProductsQuery", &products(&search, &cursor));
}
