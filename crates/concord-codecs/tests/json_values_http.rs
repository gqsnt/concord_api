//! The consecutive-values reader over real loopback HTTP/1.1 watch-style responses: values
//! arrive while the stream stays open, a number cut by a transport pause is not ended by it,
//! cancellation closes the connection, media checks precede the body, and late transport errors
//! keep the delivered prefix. Each test owns its peer.
#![cfg(feature = "jsonl")]

mod support;

use std::time::{Duration, Instant};

use concord_codecs::jsonl::{JsonValuesReader, MediaPolicy};
use concord_codecs::stream::{OpenErrorKind, ReadErrorKind};
use serde_json::{Value, json};
use support::{Peer, client, end_chunks, read_head, until_closed, write_chunk, write_head};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::timeout;

const SECRET: &str = "SECRET-7f3a91";

#[derive(Debug, PartialEq, serde::Deserialize)]
struct WatchEvent {
    #[serde(rename = "type")]
    kind: String,
    object: Value,
}

/// A watch delivers its first event while the server keeps the stream open; a number whose
/// digits are cut by a transport pause is held until a delimiter arrives (`1` … `2 ` is `12`).
#[tokio::test]
async fn values_arrive_while_the_watch_stays_open() {
    let (release, released) = oneshot::channel::<()>();
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[
                "content-type: application/json",
                "transfer-encoding: chunked",
            ],
        )
        .await;
        write_chunk(
            &mut s,
            b"{\"type\":\"ADDED\",\"object\":{\"metadata\":{\"name\":\"a\"}}}\n1",
        )
        .await;
        released.await.expect("released by the test");
        write_chunk(&mut s, b"2 {\"type\":\"DELETED\",\n\"object\":{}}").await;
        end_chunks(&mut s).await;
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut values = JsonValuesReader::new()
        .open_for::<Value>(response, "watch_pods")
        .unwrap();
    let first = timeout(Duration::from_secs(5), values.next())
        .await
        .expect("the first event arrives while the stream is open")
        .unwrap()
        .unwrap();
    assert_eq!(first["type"], "ADDED");
    assert!(
        timeout(Duration::from_millis(100), values.next())
            .await
            .is_err(),
        "`1` is not a complete value while more digits may follow"
    );
    assert_eq!(values.retained_bytes(), 1, "the digit waits in the buffer");
    release.send(()).unwrap();
    assert_eq!(values.next().await.unwrap().unwrap(), json!(12));
    assert_eq!(
        values.next().await.unwrap().unwrap(),
        json!({"type": "DELETED", "object": {}})
    );
    assert!(values.next().await.is_none());
    drop(values);
    drop(client);
    peer.join().await;
}

/// Dropping the values while waiting for the next event closes the connection promptly (the
/// client itself stays alive).
#[tokio::test]
async fn drop_while_watching_releases_the_connection() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[
                "content-type: application/json",
                "transfer-encoding: chunked",
            ],
        )
        .await;
        write_chunk(&mut s, b"{\"type\":\"ADDED\",\"object\":{}}\n").await;
        until_closed(&mut s).await
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut values = JsonValuesReader::new()
        .open::<WatchEvent>(response)
        .unwrap();
    assert_eq!(values.next().await.unwrap().unwrap().kind, "ADDED");
    assert!(
        timeout(Duration::from_millis(50), values.next())
            .await
            .is_err()
    );
    let dropped = Instant::now();
    drop(values);
    peer.join().await;
    assert!(dropped.elapsed() < Duration::from_secs(5));
    drop(client);
}

async fn open_with(
    policy: MediaPolicy,
    content_type: Option<&'static str>,
) -> Result<Value, (OpenErrorKind, &'static str)> {
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        let mut headers = vec!["content-length: 7".to_owned()];
        if let Some(ct) = content_type {
            headers.push(format!("content-type: {ct}"));
        }
        let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
        write_head(&mut s, "200 OK", &headers).await;
        s.write_all(b"{\"a\":1}").await.unwrap();
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let result = match JsonValuesReader::new()
        .media(policy)
        .open::<Value>(response)
    {
        Err(e) => Err((e.kind(), e.codec().map_or("", |c| c.label()))),
        Ok(mut values) => Ok(values.next().await.unwrap().unwrap()),
    };
    drop(client);
    peer.join().await;
    result
}

/// The default media policy accepts JSON (with parameters such as Kubernetes' `stream=watch`)
/// and refuses other types before the body is read.
#[tokio::test]
async fn watch_media_policy() {
    let value = Ok(json!({"a": 1}));
    let standard = MediaPolicy::standard();
    assert_eq!(open_with(standard, Some("application/json")).await, value);
    assert_eq!(
        open_with(standard, Some("application/json;stream=watch")).await,
        value
    );
    assert_eq!(
        open_with(standard, Some("application/json; charset=utf-8")).await,
        value
    );
    assert_eq!(
        open_with(standard, Some("application/vnd.acme+json")).await,
        value
    );
    let refused = (
        OpenErrorKind::Metadata,
        "the response Content-Type is not application/json or an application/*+json type",
    );
    assert_eq!(
        open_with(
            standard,
            Some("application/vnd.kubernetes.protobuf;stream=watch")
        )
        .await,
        Err(refused)
    );
    assert_eq!(open_with(standard, Some("text/plain")).await, Err(refused));
    assert_eq!(
        open_with(standard, None).await,
        Err((OpenErrorKind::Metadata, "the response has no Content-Type"))
    );
    assert_eq!(open_with(MediaPolicy::any(), None).await, value);
}

/// A connection lost after delivered events is a `Transport` error; the events stay delivered
/// and the reader is fused (it does not reconnect: that is the application's job).
#[tokio::test]
async fn late_transport_error_keeps_delivered_values() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &["content-type: application/json", "content-length: 500"],
        )
        .await;
        s.write_all(b"{\"type\":\"ADDED\",\"object\":{}}\n{\"type\":\"MODIFIED\",\"object\":{}}\n{\"type\":")
            .await
            .unwrap();
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut values = JsonValuesReader::new()
        .open_for::<WatchEvent>(response, "watch_pods")
        .unwrap();
    assert_eq!(values.next().await.unwrap().unwrap().kind, "ADDED");
    assert_eq!(values.next().await.unwrap().unwrap().kind, "MODIFIED");
    let error = values.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.position().records, 2);
    assert!(
        error.to_string().starts_with("watch_pods: reading failed"),
        "{error}"
    );
    assert!(values.next().await.is_none());
    drop(values);
    drop(client);
    peer.join().await;
}

/// A value of the wrong shape is a `Decode` error whose diagnostics never quote the value.
#[tokio::test]
async fn value_diagnostics_never_show_content() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        let body = format!("{{\"type\":\"ADDED\",\"object\":{{}}}} {{\"type\":{{\"token\":\"{SECRET}\"}},\"object\":1}}");
        let length = format!("content-length: {}", body.len());
        write_head(&mut s, "200 OK", &["content-type: application/json", &length]).await;
        s.write_all(body.as_bytes()).await.unwrap();
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut values = JsonValuesReader::new()
        .open_for::<WatchEvent>(response, "watch_pods")
        .unwrap();
    assert_eq!(values.next().await.unwrap().unwrap().kind, "ADDED");
    let error = values.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    let shown = format!("{error} | {error:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    assert!(
        shown.contains("a JSON value does not match the expected type"),
        "{shown}"
    );
    assert!(error.source_unredacted().is_some());
    assert!(values.next().await.is_none());
    drop(values);
    drop(client);
    peer.join().await;
}
