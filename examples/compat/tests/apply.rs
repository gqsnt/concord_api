//! Loopback compatibility evidence for apply.
mod support;
use concord_example_compat::Apply;
use support::{http, Reply, Server};

#[tokio::test]
async fn json_with_apply_yaml_media_type() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Apply::new(http()).with_base_url(server.url()).unwrap();
    api.patch(
        "compat manager",
        serde_json::json!({"apiVersion":"v1","kind":"ConfigMap"}),
    )
    .send()
    .await
    .unwrap();
    let r = server.single();
    assert_eq!(r.method, "PATCH");
    assert_eq!(
        r.target,
        "/api/v1/namespaces/default/configmaps/demo?fieldManager=compat+manager"
    );
    assert_eq!(
        r.header("content-type"),
        Some("application/apply-patch+yaml")
    );
    assert_eq!(r.body, br#"{"apiVersion":"v1","kind":"ConfigMap"}"#);
}
