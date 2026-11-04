//! The native consumer must distinguish refusal of the head from body decoding.

use std::time::Duration;

use concord::{ErrorKind, reqwest};
use concord_example_native::api::{Inventory, InventoryConfig};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::oneshot,
};

const LIMIT: Duration = Duration::from_secs(3);
const BODY: &[u8] = br#"{"id":7,"name":"Pen"}"#;

async fn fixture(
    status: u16,
    content_type: &'static str,
    body: &'static [u8],
) -> (Inventory, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (release, wait) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(LIMIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            let n = tokio::time::timeout(LIMIT, socket.read(&mut byte))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(n, 1);
            request.push(byte[0]);
        }
        let head = format!(
            "HTTP/1.1 {status} Test\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        if tokio::time::timeout(LIMIT * 2, wait)
            .await
            .is_ok_and(|v| v.is_ok())
        {
            let _ = socket.write_all(body).await;
        }
    });
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(LIMIT * 2)
        .build()
        .unwrap();
    let api = Inventory::new(
        client,
        InventoryConfig {
            key: "dummy".into(),
        },
    )
    .with_base_url(format!("http://{addr}"))
    .unwrap();
    (api, release, task)
}

#[tokio::test]
async fn refused_metadata_does_not_wait_for_body_and_is_recoverable() {
    let (api, release, server) = fixture(200, "text/html", BODY).await;
    let result = tokio::time::timeout(LIMIT, api.item(7).send()).await;
    assert!(
        result.is_ok(),
        "expected Metadata before body; timed out waiting for withheld body"
    );
    let error = result.unwrap().unwrap_err();
    assert_eq!(
        error.kind(),
        ErrorKind::Metadata,
        "expected Metadata before body"
    );
    let response = error.into_response().unwrap();
    assert_eq!(response.headers()["content-type"], "text/html");
    release.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(LIMIT, response.bytes())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        BODY
    );
    tokio::time::timeout(LIMIT, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn valid_head_and_body_decode_and_invalid_body_is_decode() {
    for (body, valid) in [(BODY, true), (b"not json" as &'static [u8], false)] {
        let (api, release, server) = fixture(200, "application/json", body).await;
        release.send(()).unwrap();
        let result = tokio::time::timeout(LIMIT, api.item(7).send())
            .await
            .unwrap();
        if valid {
            assert_eq!(result.unwrap().name, "Pen");
        } else {
            assert_eq!(result.unwrap_err().kind(), ErrorKind::Decode);
        }
        tokio::time::timeout(LIMIT, server).await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn status_response_is_recoverable() {
    let (api, release, server) = fixture(409, "text/html", BODY).await;
    let error = tokio::time::timeout(LIMIT, api.item(7).send())
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    let response = error.into_response().unwrap();
    assert_eq!(response.status(), 409);
    release.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(LIMIT, response.bytes())
            .await
            .unwrap()
            .unwrap()
            .as_ref(),
        BODY
    );
    tokio::time::timeout(LIMIT, server).await.unwrap().unwrap();
}
