//! Loopback compatibility evidence for application.
mod support;
use concord::ErrorKind;
use concord_example_compat::Application;
use support::{http, Reply, Server};

#[tokio::test]
async fn caller_or_custom_decoder_checks_the_envelope() {
    let server = Server::reply(Reply::json(
        200,
        r#"{"ok":false,"error":"synthetic-detail"}"#,
    ))
    .await;
    let api = Application::new(http())
        .with_base_url(server.url())
        .unwrap();
    let envelope = api.explicit().send().await.unwrap();
    assert!(!envelope.ok);
    assert_eq!(envelope.error.as_deref(), Some("synthetic-detail"));
    let error = api.checked().send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert_eq!(error.body_source().unwrap().to_string(), "synthetic-detail");
    assert!(error.to_string().ends_with("application rejected request"));
    assert!(!format!("{error} {error:?}").contains("synthetic-detail"));
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        ["/explicit", "/checked"]
    );

    let server = Server::reply(Reply::json(200, r#"{"ok":true,"error":null}"#)).await;
    let api = Application::new(http())
        .with_base_url(server.url())
        .unwrap();
    assert!(api.checked().send().await.unwrap().ok);
}
