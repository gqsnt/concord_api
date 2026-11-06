//! Fresh consumer workflows against a local Open Library fixture.

use std::{num::NonZeroU32, time::Duration};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_open_library::{Error, OpenLibrary, concord::reqwest};

fn client(server: &TestServer) -> OpenLibrary {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .user_agent("first-use-fixture/1.0")
        .build()
        .unwrap();
    OpenLibrary::new(http).with_base_url(server.url()).unwrap()
}

fn assert_request(request: &Recorded, server: &TestServer, path: &str, pairs: &[(&str, &str)]) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    let mut actual = request.query_pairs();
    actual.sort();
    let mut expected: Vec<_> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(request.host, server.addr().to_string());
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("user-agent"), Some("first-use-fixture/1.0"));
    assert_eq!(request.header("authorization"), None);
    assert_eq!(request.body_len, 0);
}

#[tokio::test]
async fn search_work_select_retrieve_and_observe_empty_or_bad_page() {
    let server = TestServer::start(|r: &Recorded| match (r.path(), r.query_all("page").first().map(String::as_str)) {
        ("/search.json", Some("2")) => Reply::json(200, r#"{"numFound":5,"start":2,"numFoundExact":false,"docs":[{"key":"/works/OL27448W","title":"The Lord of the Rings","author_name":["J. R. R. Tolkien"],"first_publish_year":1954}]}"#),
        ("/works/OL27448W.json", _) => Reply::json(200, r#"{"key":"/works/OL27448W","title":"The Lord of the Rings"}"#),
        ("/search.json", Some("3")) => Reply::json(200, r#"{"num_found":5,"start":4,"docs":[]}"#),
        ("/search.json", Some("4")) => Reply::json(200, r#"{"numFound":5,"docs":{}}"#),
        _ => Reply::json(404, r#"{"error":"unexpected request"}"#),
    }).await;
    let api = client(&server);
    let query = "the lord of the rings";
    let size = NonZeroU32::new(2).unwrap();
    let request = api
        .search()
        .works(query)
        .set_page(NonZeroU32::new(2).unwrap())
        .set_limit(size);
    assert!(!format!("{request:?}").contains(query));
    let page = request.send().await.unwrap();
    assert_eq!(
        (page.num_found, page.start, page.num_found_exact),
        (Some(5), Some(2), Some(false))
    );
    assert_eq!(page.docs.len(), 1);
    let work = api
        .works()
        .get(page.docs[0].id().unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(work.title.as_deref(), Some("The Lord of the Rings"));
    assert!(work.description.is_none() && work.subjects.is_none());

    let empty = api
        .search()
        .works(query)
        .set_page(NonZeroU32::new(3).unwrap())
        .set_limit(size)
        .send()
        .await
        .unwrap();
    assert!(empty.docs.is_empty());
    assert_eq!((empty.num_found, empty.start), (Some(5), Some(4)));
    // Stop on this observed empty page; the count is not a snapshot guarantee.
    let bad = api
        .search()
        .works(query)
        .set_page(NonZeroU32::new(4).unwrap())
        .set_limit(size)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(bad, Error::Concord(_)));
    assert_eq!(bad.status().unwrap().as_u16(), 200);
    // The already selected work remains available after the later page failed.
    assert_eq!(work.key.as_deref(), Some("/works/OL27448W"));

    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_request(
        &requests[0],
        &server,
        "/search.json",
        &[("q", query), ("page", "2"), ("limit", "2")],
    );
    assert_request(&requests[1], &server, "/works/OL27448W.json", &[]);
    assert_request(
        &requests[2],
        &server,
        "/search.json",
        &[("q", query), ("page", "3"), ("limit", "2")],
    );
    assert_request(
        &requests[3],
        &server,
        "/search.json",
        &[("q", query), ("page", "4"), ("limit", "2")],
    );
    server.shutdown().await;
}

#[tokio::test]
async fn search_author_retrieve_works_and_keep_partial_batch_on_error() {
    let server = TestServer::start(|r: &Recorded| match (r.path(), r.query_all("offset").first().map(String::as_str)) {
        ("/search/authors.json", _) => Reply::json(200, r#"{"numFound":1,"start":0,"numFoundExact":true,"docs":[{"key":"OL23919A","name":"J. K. Rowling","work_count":162}]}"#),
        ("/authors/OL23919A/works.json", Some("0")) => Reply::json(200, r#"{"size":162,"entries":[{"key":"/works/OL82563W","title":"Harry Potter and the Philosopher's Stone"},{"key":"/works/OL66554W"}]}"#),
        ("/authors/OL23919A/works.json", Some("2")) => Reply::json(429, r#"{"message":"try later; secret fixture-token"}"#).header("retry-after", "7"),
        _ => Reply::json(404, r#"{"error":"unexpected request"}"#),
    }).await;
    let api = client(&server);
    let query = "j k rowling";
    let authors = api.search().authors(query).send().await.unwrap();
    assert_eq!(
        (authors.num_found, authors.start, authors.num_found_exact),
        (Some(1), Some(0), Some(true))
    );
    let hit = &authors.docs[0];
    assert_eq!(hit.name.as_deref(), Some("J. K. Rowling"));
    assert_eq!(hit.work_count, Some(162));
    let id = hit.id().unwrap();
    let first = api
        .author(id.clone())
        .works()
        .set_limit(NonZeroU32::new(2).unwrap())
        .set_offset(0u64)
        .send()
        .await
        .unwrap();
    assert_eq!(first.size, Some(162));
    assert_eq!(first.entries.len(), 2);
    assert!(first.entries[1].title.is_none());
    let err = api
        .author(id)
        .works()
        .set_limit(NonZeroU32::new(2).unwrap())
        .set_offset(2u64)
        .send()
        .await
        .unwrap_err();
    assert_eq!(err.status().unwrap().as_u16(), 429);
    assert!(!format!("{err:?} {err}").contains("fixture-token"));
    match &err {
        Error::Http(http) => {
            assert_eq!(http.meta().headers()["retry-after"], "7");
            assert_eq!(
                http.document_unredacted().unwrap()["message"],
                "try later; secret fixture-token"
            );
        }
        other => panic!("expected HTTP error, got {other:?}"),
    }
    // Do not claim completion: retain the first batch and its failed continuation.
    assert_eq!(
        first.entries[0].title.as_deref(),
        Some("Harry Potter and the Philosopher's Stone")
    );
    assert_eq!(first.entries.len(), 2);

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_request(
        &requests[0],
        &server,
        "/search/authors.json",
        &[("q", query)],
    );
    assert_request(
        &requests[1],
        &server,
        "/authors/OL23919A/works.json",
        &[("limit", "2"), ("offset", "0")],
    );
    assert_request(
        &requests[2],
        &server,
        "/authors/OL23919A/works.json",
        &[("limit", "2"), ("offset", "2")],
    );
    server.shutdown().await;
}
