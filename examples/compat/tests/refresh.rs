//! Loopback compatibility evidence for refresh.
mod support;
use concord_example_compat::{Refresh, RefreshConfig};
use support::{http, Reply, Server};

#[tokio::test]
async fn scopes_capture_base_and_root_values() {
    let old_server = Server::reply(Reply::new(204)).await;
    let new_server = Server::reply(Reply::new(204)).await;
    let old = Refresh::new(
        http(),
        RefreshConfig {
            token: "old-token".into(),
        },
    )
    .with_base_url(old_server.url())
    .unwrap();
    let old_scope = old.account();
    let new = old
        .clone()
        .with_base_url(new_server.url())
        .unwrap()
        .set_token("new-token");
    old_scope.get().send().await.unwrap();
    new.account().get().send().await.unwrap();
    old.account().get().send().await.unwrap();
    let old_requests = old_server.requests();
    assert_eq!(old_requests.len(), 2);
    for r in old_requests {
        assert_eq!(r.target, "/account/value");
        assert_eq!(r.header("authorization"), Some("old-token"));
        assert_eq!(r.header("host"), Some(old_server.addr.to_string().as_str()));
    }
    let r = new_server.single();
    assert_eq!(r.target, "/account/value");
    assert_eq!(r.header("authorization"), Some("new-token"));
    assert_eq!(r.header("host"), Some(new_server.addr.to_string().as_str()));
}
