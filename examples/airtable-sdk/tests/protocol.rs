#![allow(missing_docs)]
use concord::bytes::Bytes;
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_airtable::{
    Client, ERROR_BODY_LIMIT, Error, ListOptions, ProviderError, SUCCESS_BODY_LIMIT,
};

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

#[tokio::test]
async fn get_encodes_path_query_and_no_body() {
    let server =
        TestServer::start(|_| Reply::json(200, include_bytes!("../fixtures/filtered.json"))).await;
    let options = ListOptions {
        fields: vec!["Old name".into(), "New & name".into(), "Unknown".into()],
        filter_by_formula: Some("{New & name} = 'a+b /?'".into()),
        page_size: Some(50),
        max_records: Some(90),
        view: Some("Grid view".into()),
    };
    let response = client(&server)
        .list_page("app example", "Tasks /?", &options, Some("itr/rec+?&=% #"))
        .await
        .unwrap();
    assert_eq!(response.value.records[0].id, "recFiltered");
    let requests = server.requests();
    let r = &requests[0];
    assert_eq!(r.method, "GET");
    assert_eq!(
        r.target,
        "/v0/app%20example/Tasks%20%2F%3F?fields%5B%5D=Old+name&fields%5B%5D=New+%26+name&fields%5B%5D=Unknown&filterByFormula=%7BNew+%26+name%7D+%3D+%27a%2Bb+%2F%3F%27&offset=itr%2Frec%2B%3F%26%3D%25+%23&pageSize=50&maxRecords=90&view=Grid+view"
    );
    assert_eq!(
        r.header("authorization"),
        Some("Bearer SYNTHETIC_TOKEN_NOT_A_CREDENTIAL")
    );
    assert_eq!(r.header("accept"), Some("application/json"));
    assert!(r.header("content-type").is_none());
    assert_eq!(r.body, b"");
    assert_eq!(r.body_len, 0);
    assert_eq!(
        r.query_all("fields[]"),
        ["Old name", "New & name", "Unknown"]
    );
    assert_eq!(r.query_all("offset"), ["itr/rec+?&=% #"]);
    server.shutdown().await;
}

#[tokio::test]
async fn provider_errors_keep_metadata_and_document_forms() {
    let server = TestServer::start(|r| {
        if r.path().ends_with("missing") {
            Reply::json(404, include_bytes!("../fixtures/error-string.json"))
        } else {
            Reply::json(403, include_bytes!("../fixtures/error-object.json"))
                .header("retry-after", "3")
                .header("x-request-id", "synthetic-request")
        }
    })
    .await;
    let api = client(&server);
    let err = api
        .list_page("app", "denied", &ListOptions::default(), None)
        .await
        .unwrap_err();
    assert_eq!(err.meta().unwrap().status().as_u16(), 403);
    assert_eq!(err.meta().unwrap().headers()["retry-after"], "3");
    assert!(err.meta().unwrap().operation().is_some());
    assert!(
        matches!(err.provider(), Some(ProviderError::Object { kind, message: Some(_) }) if kind == "INVALID_PERMISSIONS_OR_MODEL_NOT_FOUND")
    );
    assert!(!format!("{err:?}").contains("Synthetic permission"));
    let err = api
        .list_page("app", "missing", &ListOptions::default(), None)
        .await
        .unwrap_err();
    assert!(matches!(err.provider(), Some(ProviderError::String(kind)) if kind == "NOT_FOUND"));
    server.shutdown().await;
}

#[tokio::test]
async fn invalid_input_and_bad_responses_never_become_exhaustion() {
    let server = TestServer::start(|r| match r.path() {
        "/v0/app/empty" => Reply::json(200, include_bytes!("../fixtures/invalid-page.json")),
        "/v0/app/bad" => Reply::json(200, b"not json"),
        "/v0/app/null" => Reply::json(200, br#"{"records":[],"offset":null}"#),
        "/v0/app/html" => Reply::text(503, "text/html", b"failure"),
        "/v0/app/broken" => Reply::json(200, b"")
            .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"records\":[")])),
        "/v0/app/large-error" => Reply::json(429, vec![b'x'; ERROR_BODY_LIMIT + 1]),
        "/v0/app/large" => Reply::json(200, vec![b' '; SUCCESS_BODY_LIMIT + 1]),
        _ => panic!("unexpected request"),
    })
    .await;
    let api = client(&server);
    let opts = ListOptions::default();
    assert!(matches!(
        api.list_page("app", "..", &opts, None).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.list_page(
            "app",
            "x",
            &ListOptions {
                page_size: Some(0),
                ..Default::default()
            },
            None
        )
        .await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        api.list_page("app", "x", &opts, Some("")).await,
        Err(Error::InvalidInput(_))
    ));
    assert!(server.requests().is_empty());
    assert!(matches!(
        api.list_page("app", "empty", &opts, None).await,
        Err(Error::EmptyOffset { .. })
    ));
    for name in ["bad", "null", "broken", "large"] {
        let error = api.list_page("app", name, &opts, None).await.unwrap_err();
        assert!(matches!(error, Error::Concord { meta: Some(_), .. }));
    }
    let error = api.list_page("app", "html", &opts, None).await.unwrap_err();
    assert!(matches!(
        error,
        Error::Http {
            provider: None,
            body: Some(_),
            ..
        }
    ));
    let error = api
        .list_page("app", "large-error", &opts, None)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        Error::Http {
            body: None,
            body_error: Some(_),
            ..
        }
    ));
    server.shutdown().await;
}
