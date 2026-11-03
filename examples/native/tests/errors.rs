//! U14: a non-2xx response with a problem document.

mod support;

use concord::ErrorKind;
use concord_example_native::errors::{Problem, RenameError, rename_item};
use support::{Reply, Server, inventory_at};

#[tokio::test]
async fn problem_document_keeps_status_and_body() {
    let server = Server::reply(
        Reply::new(409)
            .header("content-type", "application/problem+json")
            .body(r#"{"type":"urn:inventory:locked","title":"Item locked","detail":"item 7 is being audited"}"#),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = rename_item(&api, 7, "Pen").await.unwrap_err();
    let RenameError::Problem { status, problem } = error else {
        panic!("expected a problem, got {error:?}");
    };
    assert_eq!(status.as_u16(), 409);
    assert_eq!(
        problem,
        Problem {
            kind: Some("urn:inventory:locked".into()),
            title: "Item locked".into(),
            detail: Some("item 7 is being audited".into()),
        }
    );
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("PUT", "/v1/items/7")
    );
    assert_eq!(request.body, br#"{"name":"Pen"}"#);
}

#[tokio::test]
async fn success_is_the_declared_item() {
    let server = Server::reply(Reply::json(200, r#"{"id":7,"name":"Pen"}"#)).await;
    let item = rename_item(&inventory_at(server.addr, "Bearer t"), 7, "Pen")
        .await
        .unwrap();
    assert_eq!(item.name, "Pen");
}

#[tokio::test]
async fn non_problem_error_body_keeps_status_and_a_content_free_reason() {
    let server = Server::reply(Reply::text(502, "<html>secret upstream page</html>")).await;
    let error = rename_item(&inventory_at(server.addr, "Bearer t"), 7, "Pen")
        .await
        .unwrap_err();
    let RenameError::ProblemUndecodable { status, error } = &error else {
        panic!("expected undecodable, got {error:?}");
    };
    assert_eq!(status.as_u16(), 502);
    assert_eq!(error.label(), "the body is not valid JSON", "{error}");
    assert!(!error.to_string().contains("secret"));
}

#[tokio::test]
async fn non_status_errors_propagate_unchanged() {
    // A port with nothing listening: a transport error, not a status error.
    let addr = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap()
    };
    let error = rename_item(&inventory_at(addr, "Bearer t"), 7, "Pen")
        .await
        .unwrap_err();
    let RenameError::Concord(error) = error else {
        panic!("expected Concord error, got {error:?}");
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.endpoint(), Some("Inventory.rename"));
}
