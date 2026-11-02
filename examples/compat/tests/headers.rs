//! Loopback compatibility evidence for headers.
mod support;
use concord_example_compat::{Flat, Headers, HeadersConfig, Relabel};
use support::{http, Reply, Server};

const ROOT: &str = "synthetic-root-secret";
const SECRET: &str = "synthetic-request-secret";
fn redacted(shown: String) {
    assert!(!shown.contains(ROOT), "{shown}");
    assert!(!shown.contains(SECRET), "{shown}");
}

#[tokio::test]
async fn native_identity_media_type_and_sensitivity() {
    let server = Server::reply(Reply::new(403)).await;
    let api = Headers::new(http(), HeadersConfig { token: ROOT.into() })
        .with_base_url(server.url())
        .unwrap();
    redacted(format!("{api:?}"));
    let body = Flat {
        title: "pen".into(),
        order: 7,
    };
    let removed = api.removed(SECRET, body.clone());
    let replaced = api.replaced(SECRET, body);
    redacted(format!("{removed:?} {replaced:?}"));
    for (prepared, media, target) in [
        (removed.prepare().unwrap(), "application/json", "/removed"),
        (
            replaced.prepare().unwrap(),
            "application/custom",
            "/replaced",
        ),
    ] {
        redacted(format!("{prepared:?}"));
        let headers = prepared.request().headers();
        assert_eq!(headers["authorization"], SECRET);
        assert!(headers["authorization"].is_sensitive());
        assert_eq!(headers["content-type"], media);
        assert!(!headers["content-type"].is_sensitive());
        assert_eq!(headers["x-other"], "ordinary");
        assert!(!headers["x-other"].is_sensitive());
        let error = prepared.send().await.unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), 403);
        redacted(format!("{error} {error:?}"));
        let r = server.requests().pop().unwrap();
        assert_eq!(r.target, target);
        assert_eq!(r.header("authorization"), Some(SECRET));
        assert_eq!(r.header("content-type"), Some(media));
        assert_eq!(r.header("x-other"), Some("ordinary"));
        assert_eq!(r.body, br#"{"title":"pen","order":7}"#);
    }
    assert_eq!(server.requests().len(), 2);
}

/// An inherited literal `content-type` and an optional body: the header is sent whether or not
/// the body is, and labels the JSON body when it is.
#[tokio::test]
async fn inherited_literal_content_type_with_an_optional_body() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Relabel::new(http()).with_base_url(server.url()).unwrap();
    api.maybe().send().await.unwrap();
    api.maybe()
        .set_value(Flat {
            title: "pen".into(),
            order: 7,
        })
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    for r in &requests {
        assert_eq!(
            r.header("content-type"),
            Some("application/vnd.compat+json")
        );
    }
    assert!(requests[0].body.is_empty());
    assert_eq!(requests[1].body, br#"{"title":"pen","order":7}"#);
}
