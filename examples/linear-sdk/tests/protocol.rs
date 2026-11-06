//! Wire contract and failure cases.
use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_pagination::{Outcome, PagesError, Resume};
use concord_sdk_linear::{Error, Linear};
use serde_json::{Value, json};

async fn setup(replies: Vec<Reply>, oauth: bool) -> (TestServer, Linear) {
    let script = Script::new(replies);
    let server = TestServer::start(move |_| script.next()).await;
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap();
    let client = if oauth {
        Linear::oauth(http, "synthetic-value").unwrap()
    } else {
        Linear::api_key(http, "synthetic-value").unwrap()
    }
    .with_base_url(&server.url())
    .unwrap();
    (server, client)
}
#[tokio::test]
async fn lookup_wire_and_partial_diagnostics() {
    let (server, client) = setup(
        vec![
            Reply::json(200, include_bytes!("../fixtures/issue-partial.json"))
                .header("x-request-id", "synthetic-request"),
        ],
        false,
    )
    .await;
    let response = client.issue("ABC-1\"\\").await.unwrap();
    assert!(!response.is_success());
    assert_eq!(
        response.envelope.data.unwrap().issue.unwrap().description,
        None
    );
    assert_eq!(
        response.envelope.errors[0].path.as_ref().unwrap()[1],
        "description"
    );
    assert_eq!(
        response.envelope.errors[0].extensions.as_ref().unwrap()["code"],
        "FIELD_ERROR"
    );
    assert_eq!(response.meta.headers()["x-request-id"], "synthetic-request");
    assert!(!format!("{:?}", response.meta).contains("synthetic-request"));
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/graphql")
    );
    assert_eq!(request.header("authorization"), Some("synthetic-value"));
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.body, br#"{"query":"query Issue($id: String!) { issue(id: $id) { id title description dueDate } }","operationName":"Issue","variables":{"id":"ABC-1\"\\"}}"#);
    server.shutdown().await;
}
#[tokio::test]
async fn page_wire_and_oauth() {
    let (server, client) = setup(
        vec![Reply::json(
            200,
            include_bytes!("../fixtures/page-one.json"),
        )],
        true,
    )
    .await;
    let page = client
        .issue_page(10, Some("prior\"cursor".into()))
        .await
        .unwrap();
    assert_eq!(page.next.as_deref(), Some("cursor-one"));
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/graphql")
    );
    assert_eq!(
        request.header("authorization"),
        Some("Bearer synthetic-value")
    );
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.body, br#"{"query":"query Issues($first: Int!, $after: String) { issues(first: $first, after: $after) { nodes { id title description dueDate } pageInfo { hasNextPage endCursor } } }","operationName":"Issues","variables":{"first":10,"after":"prior\"cursor"}}"#);
    server.shutdown().await;
}
#[tokio::test]
async fn http_error_and_missing_data_retain_diagnostics() {
    let (server, client) = setup(
        vec![
            Reply::json(401, include_bytes!("../fixtures/errors.json"))
                .header("x-request-id", "synthetic-request"),
            Reply::json(200, include_bytes!("../fixtures/errors.json")),
        ],
        false,
    )
    .await;
    match client.issue("x").await.unwrap_err() {
        Error::Http { meta, body, errors } => {
            assert_eq!(meta.status().as_u16(), 401);
            assert_eq!(meta.headers()["x-request-id"], "synthetic-request");
            assert_eq!(body, include_bytes!("../fixtures/errors.json"));
            assert_eq!(
                errors[0].extensions.as_ref().unwrap()["code"],
                "UNAUTHENTICATED"
            );
        }
        e => panic!("{e:?}"),
    }
    match client.issue_page(5, None).await.unwrap_err() {
        Error::Page { response, .. } => assert_eq!(
            response.envelope.errors[0].message,
            "Synthetic authorization failure"
        ),
        e => panic!("{e:?}"),
    }
    server.shutdown().await;
}
#[tokio::test]
async fn malformed_page_and_bad_responses() {
    let (server, client) = setup(
        vec![
            Reply::json(200, include_bytes!("../fixtures/invalid-page.json")),
            Reply::json(200, br#"{"data":{"issue":{"id":"x","title":"t"}}}"#),
            Reply::new(200)
                .header("content-type", "text/plain")
                .body("not json"),
            Reply::json(200, b"not json"),
        ],
        false,
    )
    .await;
    match client.issue_page(3, None).await.unwrap_err() {
        Error::Page { response, reason } => {
            assert_eq!(reason, "next page has no cursor");
            assert_eq!(
                response.envelope.errors[0].message,
                "Synthetic page diagnostic"
            );
        }
        e => panic!("{e:?}"),
    }
    assert!(matches!(
        client.issue("x").await,
        Err(Error::Response { .. })
    ));
    assert!(matches!(
        client.issue("x").await,
        Err(Error::Response { .. })
    ));
    assert!(matches!(
        client.issue("x").await,
        Err(Error::Response { .. })
    ));
    server.shutdown().await;
}
#[tokio::test]
async fn invalid_arguments_do_not_send() {
    let (server, client) = setup(vec![], false).await;
    assert!(matches!(client.issue("").await, Err(Error::Invalid(_))));
    assert!(matches!(
        client.issue_page(0, None).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        client.issue_page(251, None).await,
        Err(Error::Invalid(_))
    ));
    assert!(Linear::api_key(reqwest::Client::new(), "bad\nvalue").is_err());
    assert!(server.requests().is_empty());
    server.shutdown().await;
}
#[tokio::test]
async fn repeated_cursor_is_refused_with_data() {
    let (server, client) = setup(
        vec![Reply::json(
            200,
            include_bytes!("../fixtures/page-one.json"),
        )],
        false,
    )
    .await;
    match client
        .issue_page(1, Some("cursor-one".into()))
        .await
        .unwrap_err()
    {
        Error::Page { response, reason } => {
            assert_eq!(reason, "cursor did not advance");
            assert_eq!(
                response.envelope.data.unwrap().issues.unwrap().nodes[0].id,
                "issue-one"
            );
        }
        e => panic!("{e:?}"),
    }
    server.shutdown().await;
}
#[tokio::test]
async fn body_limits_keep_response_metadata() {
    let (server, client) = setup(
        vec![
            Reply::json(200, vec![b' '; 1024 * 1024 + 1])
                .header("x-request-id", "synthetic-large-success"),
            Reply::json(500, vec![b' '; 65537]).header("x-request-id", "synthetic-large-error"),
        ],
        false,
    )
    .await;
    for expected in ["synthetic-large-success", "synthetic-large-error"] {
        match client.issue("x").await.unwrap_err() {
            Error::Response { meta, source } => {
                assert_eq!(meta.headers()["x-request-id"], expected);
                assert_eq!(source.kind(), concord::ErrorKind::Limit);
            }
            other => panic!("{other:?}"),
        }
    }
    server.shutdown().await;
}
#[tokio::test]
async fn due_date_field_error_keeps_data_diagnostics_and_presence_rule() {
    let (server, client) = setup(
        vec![
            Reply::json(
                200,
                include_bytes!("../fixtures/issue-due-date-partial.json"),
            )
            .header("x-request-id", "synthetic-due-date"),
            Reply::json(
                200,
                br#"{"data":{"issue":{"id":"x","title":"t","description":null}}}"#,
            ),
        ],
        false,
    )
    .await;
    let response = client.issue("ABC-1").await.unwrap();
    assert!(!response.is_success());
    assert_eq!(response.meta.status(), 200);
    assert_eq!(
        response.meta.headers()["x-request-id"],
        "synthetic-due-date"
    );
    let issue = response.envelope.data.unwrap().issue.unwrap();
    assert_eq!(
        (issue.description.as_deref(), issue.due_date),
        (Some("First description"), None)
    );
    let errors = &response.envelope.errors;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "Synthetic due date unavailable");
    assert_eq!(errors[0].path, Some(vec![json!("issue"), json!("dueDate")]));
    assert_eq!(
        errors[0].locations,
        Some(vec![json!({"line":1,"column":67})])
    );
    assert_eq!(
        errors[0].extensions.as_ref().unwrap()["code"],
        "SYNTHETIC_DUE_DATE_FAILURE"
    );
    assert_eq!(
        response.envelope.extensions.unwrap()["trace"],
        "synthetic-due-date"
    );
    // A selected nullable field that is absent is a decode failure, never a null.
    match client.issue("x").await.unwrap_err() {
        Error::Response { meta, source } => {
            assert_eq!(meta.status().as_u16(), 200);
            assert_eq!(source.kind(), concord::ErrorKind::Decode);
        }
        e => panic!("{e:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body, br#"{"query":"query Issue($id: String!) { issue(id: $id) { id title description dueDate } }","operationName":"Issue","variables":{"id":"ABC-1"}}"#);
    // The fixture's location (line 1, column 67) points at `dueDate` in the sent document.
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(&body["query"].as_str().unwrap()[66..73], "dueDate");
    server.shutdown().await;
}
#[tokio::test]
async fn null_connection_is_a_page_error_that_keeps_its_diagnostics() {
    let (server, client) = setup(
        vec![
            Reply::json(200, include_bytes!("../fixtures/page-one.json")),
            Reply::json(200, include_bytes!("../fixtures/connection-null.json"))
                .header("x-request-id", "synthetic-null-connection"),
            Reply::json(200, br#"{"data":{"issues":null}}"#),
            Reply::json(
                200,
                br#"{"data":{},"errors":[{"message":"Synthetic connection failure","path":["issues"]}]}"#,
            ),
        ],
        false,
    )
    .await;
    // A GraphQL error at `["issues"]` nulls the connection: the page fails with the response.
    let mut pages = client.issues(1, Resume::Start);
    assert_eq!(
        pages.next_page().await.unwrap().unwrap().items[0].id,
        "issue-one"
    );
    let error = pages.next_page().await.unwrap_err();
    assert_eq!(error.resume(), Some(&Resume::At("cursor-one".to_owned())));
    let PagesError::Provider {
        error: Error::Page { response, reason },
        ..
    } = error
    else {
        panic!("expected a page error with the response");
    };
    assert_eq!(reason, "connection is null");
    assert_eq!(response.meta.status(), 200);
    assert_eq!(
        response.meta.headers()["x-request-id"],
        "synthetic-null-connection"
    );
    assert!(response.envelope.data.unwrap().issues.is_none());
    let errors = &response.envelope.errors;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "Synthetic connection failure");
    assert_eq!(errors[0].path, Some(vec![json!("issues")]));
    assert_eq!(
        errors[0].locations,
        Some(vec![json!({"line":1,"column":46})])
    );
    assert_eq!(
        errors[0].extensions.as_ref().unwrap()["code"],
        "SYNTHETIC_CONNECTION_ERROR"
    );
    assert_eq!(
        response.envelope.extensions.unwrap()["trace"],
        "synthetic-null-connection"
    );
    assert!(matches!(
        pages.next_page().await,
        Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        })
    ));
    // A null connection without a diagnostic is still a page error, never an empty page.
    match client.issue_page(1, None).await.unwrap_err() {
        Error::Page { response, reason } => {
            assert_eq!(reason, "connection is null");
            assert!(response.envelope.data.unwrap().issues.is_none());
            assert!(response.envelope.errors.is_empty());
        }
        e => panic!("{e:?}"),
    }
    // An absent `issues` member is not a GraphQL partial-data shape: still a decode error.
    match client.issue_page(1, None).await.unwrap_err() {
        Error::Response { meta, source } => {
            assert_eq!(meta.status().as_u16(), 200);
            assert_eq!(source.kind(), concord::ErrorKind::Decode);
        }
        e => panic!("{e:?}"),
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert!(
        std::str::from_utf8(&requests[1].body)
            .unwrap()
            .contains("\"after\":\"cursor-one\"")
    );
    // The fixture's location (line 1, column 46) points at `issues` in the sent document.
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(&body["query"].as_str().unwrap()[45..51], "issues");
    server.shutdown().await;
}
