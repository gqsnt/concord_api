//! `send P;`: every generated `send()` returns the SDK's own result through one policy type.
#![allow(missing_docs)]

use concord::reqwest::header::HeaderValue;
use concord::{Endpoint, ErrorKind, SendWith};
use concord_example_testserver::{Reply, TestServer};
use serde::Deserialize;

#[derive(Debug, PartialEq, Deserialize)]
pub struct Item {
    pub id: u64,
}

/// The SDK's metadata and error, returned by every endpoint's `send()`.
#[derive(Debug, PartialEq)]
pub struct Meta {
    pub request_id: Option<String>,
}

#[derive(Debug)]
pub enum SdkError {
    Concord(concord::Error, Option<Meta>),
    Provider {
        status: u16,
        document: String,
        meta: Meta,
    },
}

fn meta(head: &concord::ResponseHead<'_>) -> Meta {
    Meta {
        request_id: head
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned),
    }
}

/// A policy that classifies provider errors and keeps metadata.
pub struct Policy;

impl<T: Send> SendWith<T> for Policy {
    type Output = Result<(T, Meta), SdkError>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare().map_err(|e| SdkError::Concord(e, None))?;
        let received = prepared
            .receive()
            .await
            .map_err(|e| SdkError::Concord(e, None))?;
        let meta = meta(&received.head());
        if !received.status().is_success() {
            let status = received.status().as_u16();
            let bytes = received
                .bytes_limited(1024)
                .await
                .map_err(|e| SdkError::Concord(e, Some(Meta { request_id: None })))?;
            return Err(SdkError::Provider {
                status,
                document: String::from_utf8_lossy(&bytes).into_owned(),
                meta,
            });
        }
        match received.decode().await {
            Ok(value) => Ok((value, meta)),
            Err(e) => Err(SdkError::Concord(e, Some(meta))),
        }
    }
}

concord::api! {
    pub client Store {
        base "http://store.invalid";
        send Policy;
        format concord::Json;
        GET item(id: u64) -> Item { path "items/{id}"; }
        GET events() -> stream { path "events"; }
    }
}

/// A policy that signs the built request (the S3 case): the signature covers the final URL, so it
/// is computed after `prepare()`, and the generated `send()` of a public declaration is signed.
pub struct Signed;

impl<T: Send> SendWith<T> for Signed {
    type Output = Result<T, concord::Error>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        let digest = format!("sig({} {})", request.method(), request.url().path());
        let mut value = HeaderValue::try_from(digest).expect("a header value");
        value.set_sensitive(true);
        request.headers_mut().insert("authorization", value);
        decoder.execute(&client, request).await?.decode().await
    }
}

concord::api! {
    pub client Bucket {
        base "http://bucket.invalid";
        send Signed;
        format concord::Json;
        GET object(key: String) -> Item { path "objects/{key}"; }
    }
}

#[tokio::test]
async fn every_send_returns_the_policy_output() {
    let server = TestServer::start(|r| {
        if r.target.contains("/items/7") {
            Reply::json(200, "{\"id\":7}").header("x-request-id", "r7")
        } else if r.target.contains("/items/") {
            Reply::json(404, "{\"message\":\"no such item\"}").header("x-request-id", "r404")
        } else {
            Reply::new(200)
                .header("content-type", "text/plain")
                .body("ev")
        }
    })
    .await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    let (item, meta) = store.item(7).send().await.unwrap();
    assert_eq!(item, Item { id: 7 });
    assert_eq!(meta.request_id.as_deref(), Some("r7"));
    match store.item(8).send().await.unwrap_err() {
        SdkError::Provider {
            status,
            document,
            meta,
        } => {
            assert_eq!(status, 404);
            assert!(document.contains("no such item"));
            assert_eq!(meta.request_id.as_deref(), Some("r404"));
        }
        other => panic!("{other:?}"),
    }
    // A `-> stream` endpoint through the same policy: the unread body.
    let (incoming, _) = store.events().send().await.unwrap();
    assert_eq!(incoming.drain().await.unwrap(), 2);
    // `prepare()` is unchanged (Concord's error, the native request).
    assert_eq!(store.item(9).prepare().unwrap().request().method(), "GET");
    // The future is `Send`.
    let spawned = tokio::spawn(store.item(7).send()).await.unwrap();
    assert!(spawned.is_ok());
    let dead = TestServer::start(|_| Reply::hang_up()).await;
    let store = Store::new(concord::reqwest::Client::new())
        .with_base_url(dead.url())
        .unwrap();
    let SdkError::Concord(e, None) = store.item(7).send().await.unwrap_err() else {
        panic!("transport");
    };
    assert_eq!(e.kind(), ErrorKind::Transport);
}

#[tokio::test]
async fn a_signing_policy_signs_the_built_request() {
    let server = TestServer::start(|r| {
        if r.header("authorization") == Some("sig(GET /objects/a%2Fb)") {
            Reply::json(200, "{\"id\":1}")
        } else {
            Reply::new(403)
        }
    })
    .await;
    let bucket = Bucket::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();
    assert_eq!(bucket.object("a/b").send().await.unwrap(), Item { id: 1 });
    assert_eq!(
        server.single().header("authorization"),
        Some("sig(GET /objects/a%2Fb)")
    );
}
