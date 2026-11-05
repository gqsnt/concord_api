//! The JSON Lines reader over real loopback HTTP/1.1 responses (reqwest): first record before the
//! server finishes, opening refusals that keep the body unread, late transport errors after
//! delivered records, cancellation while awaiting data, media policies and safe diagnostics.
//! Each test owns its peer (one connection, joined or aborted).
#![cfg(feature = "jsonl")]

mod support;

use std::time::{Duration, Instant};

use concord_extras::jsonl::{JsonLinesOptions, JsonLinesReader, MediaPolicy};
use concord_extras::stream::{OpenErrorKind, ReadErrorKind};
use serde_json::{Value, json};
use support::{Peer, client, end_chunks, read_head, until_closed, write_chunk, write_head};
use tokio::io::AsyncWriteExt;
use tokio::sync::oneshot;
use tokio::time::timeout;

const SECRET: &str = "SECRET-7f3a91";

#[derive(Debug, PartialEq, serde::Deserialize)]
struct Rec {
    id: u32,
}

/// The reader delivers record 1 while the server still withholds the rest of the body; a
/// record split across that pause is completed when the tail arrives (and a `next()` dropped
/// while waiting for it loses nothing).
#[tokio::test]
async fn first_record_arrives_before_the_server_finishes() {
    let (release, released) = oneshot::channel::<()>();
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[
                "content-type: application/jsonl",
                "transfer-encoding: chunked",
            ],
        )
        .await;
        write_chunk(&mut s, b"{\"id\":1}\n{\"id\":").await;
        released.await.expect("released by the test");
        write_chunk(&mut s, b"2}\n{\"id\":3}\n").await;
        end_chunks(&mut s).await;
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .open_for::<Rec>(response, "bulk_export")
        .unwrap();
    let first = timeout(Duration::from_secs(5), records.next())
        .await
        .expect("record 1 arrives while the tail is withheld")
        .unwrap()
        .unwrap();
    assert_eq!(first, Rec { id: 1 });
    assert!(
        timeout(Duration::from_millis(100), records.next())
            .await
            .is_err(),
        "record 2 needs the withheld tail"
    );
    // The cancelled `next()` left the unfinished `{"id":` in the reader's line buffer.
    assert_eq!(records.retained_bytes(), 6);
    assert_eq!(records.pending_input_bytes(), 0);
    release.send(()).unwrap();
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 2 });
    assert_eq!(records.retained_bytes(), 0, "released once parsed");
    assert_eq!(records.peak_retained_bytes(), 8, "the split line, buffered");
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 3 });
    assert!(records.next().await.is_none());
    // The body was read to its end: the idle connection is the client's (pool) until it drops.
    drop(records);
    drop(client);
    peer.join().await;
}

/// Serves a head with `Content-Length` but withholds the body until released.
async fn withheld(
    status: &'static str,
    content_type: &'static str,
    body: &'static str,
) -> (Peer<()>, oneshot::Sender<()>) {
    let (release, released) = oneshot::channel::<()>();
    let peer = Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        let length = format!("content-length: {}", body.len());
        let ct = format!("content-type: {content_type}");
        write_head(&mut s, status, &[&ct, &length]).await;
        if released.await.is_ok() {
            s.write_all(body.as_bytes()).await.unwrap();
        }
        until_closed(&mut s).await;
    })
    .await;
    (peer, release)
}

/// A non-2xx status or a refused head is an `OpenError` returned while the body has not even
/// been sent: the response comes back unread, and its whole body can still be read afterwards.
#[tokio::test]
async fn head_refusal_leaves_the_body_unread() {
    let body: &'static str = "{\"error\":\"SECRET-7f3a91\"}";
    for (status, content_type, kind) in [
        ("404 Not Found", "application/jsonl", OpenErrorKind::Status),
        (
            "200 OK",
            "text/html; note=SECRET-7f3a91",
            OpenErrorKind::Metadata,
        ),
    ] {
        let (peer, release) = withheld(status, content_type, body).await;
        let client = client();
        let response = client.get(peer.url()).send().await.unwrap();
        let error = JsonLinesReader::new(JsonLinesOptions::json_lines())
            .open_for::<Value>(response, "bulk_export")
            .unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.operation(), Some("bulk_export"));
        let shown = format!("{error} {error:?}");
        assert!(shown.starts_with("bulk_export: "), "{shown}");
        assert!(shown.contains("(response kept unread)"), "{shown}");
        assert!(!shown.contains(SECRET), "{shown}");
        if kind == OpenErrorKind::Metadata {
            assert_eq!(
                error.codec().unwrap().label(),
                "the response Content-Type is not a JSON Lines media type (application/jsonl, application/jsonlines, application/x-jsonlines)"
            );
        } else {
            assert_eq!(error.status_code().as_u16(), 404);
        }
        // Only now does the server send the body; the caller reads all of it.
        let response = error.into_response();
        release.send(()).unwrap();
        let text = timeout(Duration::from_secs(5), response.text())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(text, body);
        drop(client);
        peer.join().await;
    }
}

/// A transport failure after delivered records is a `Transport` error at the right position;
/// the delivered records stay delivered and the reader is fused. The URL (here with a synthetic
/// secret in its path) appears nowhere, not even in the unredacted source.
#[tokio::test]
async fn late_transport_error_keeps_the_prefix_and_fuses() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &["content-type: application/x-ndjson", "content-length: 1000"],
        )
        .await;
        s.write_all(b"{\"id\":1}\n\n{\"id\":2}\n").await.unwrap();
        s.flush().await.unwrap();
        // Close with 981 promised bytes missing.
    })
    .await;
    let client = client();
    let url = format!("{}{SECRET}/export", peer.url());
    let response = client.get(&url).send().await.unwrap();
    let mut records = JsonLinesReader::new(JsonLinesOptions::ndjson())
        .open_for::<Rec>(response, "bulk_export")
        .unwrap();
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 1 });
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 2 });
    let error = records.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.operation(), Some("bulk_export"));
    assert_eq!(error.position().records, 2);
    assert_eq!(error.position().bytes, 19);
    assert_eq!(
        error.to_string(),
        "bulk_export: reading failed: reading the response body failed (after 2 records, 19 bytes)"
    );
    let source = error.source_unredacted().expect("the reqwest error");
    assert!(!format!("{error:?} {source} {source:?}").contains(SECRET));
    assert!(records.next().await.is_none(), "fused");
    assert!(records.next().await.is_none(), "still fused");
    assert_eq!(records.position().records, 2);
    drop(records);
    peer.join().await;
}

/// Dropping the records while a `next()` awaits data closes the connection promptly: the server
/// sees it closed, with no task left reading on the reader's behalf.
#[tokio::test]
async fn drop_while_awaiting_data_releases_the_connection() {
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[
                "content-type: application/jsonl",
                "transfer-encoding: chunked",
            ],
        )
        .await;
        write_chunk(&mut s, b"{\"id\":1}\n{\"id\":2").await;
        until_closed(&mut s).await
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .open::<Rec>(response)
        .unwrap();
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 1 });
    let started = Instant::now();
    assert!(
        timeout(Duration::from_millis(50), records.next())
            .await
            .is_err(),
        "waiting for data that never comes"
    );
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "cancellation returned promptly"
    );
    // The client (and its connection pool) stays alive: only dropping the unfinished records
    // closes the connection.
    let dropped = Instant::now();
    drop(records);
    peer.join().await;
    let closed = dropped.elapsed();
    assert!(
        closed < Duration::from_secs(5),
        "the server saw the close after {closed:?}"
    );
    drop(client);
}

/// Serves `body` under `content_type` (or none).
async fn serve(content_type: Option<&'static str>, body: &'static [u8]) -> Peer<()> {
    Peer::start(move |mut s| async move {
        read_head(&mut s).await;
        let length = format!("content-length: {}", body.len());
        let mut headers = vec![length];
        if let Some(ct) = content_type {
            headers.push(format!("content-type: {ct}"));
        }
        let headers: Vec<&str> = headers.iter().map(String::as_str).collect();
        write_head(&mut s, "200 OK", &headers).await;
        s.write_all(body).await.unwrap();
        until_closed(&mut s).await;
    })
    .await
}

async fn open_with(
    policy: MediaPolicy,
    content_type: Option<&'static str>,
) -> Result<Vec<Value>, &'static str> {
    let peer = serve(content_type, b"{\"a\":1}\n[2]\n").await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let reader = JsonLinesReader::new(JsonLinesOptions::json_lines().media(policy));
    let result = match reader.open::<Value>(response) {
        Err(e) => Err(e.codec().map_or("status", |c| c.label())),
        Ok(mut records) => {
            let mut out = Vec::new();
            while let Some(r) = records.next().await {
                out.push(r.unwrap());
            }
            Ok(out)
        }
    };
    drop(client);
    peer.join().await;
    result
}

/// An SDK accepts a provider's actual `Content-Type` deliberately (a signed download served as
/// `application/octet-stream`); the documented types stay accepted, others stay refused, and
/// `any()` skips the check entirely.
#[tokio::test]
async fn media_policies_are_deliberate() {
    let records = Ok(vec![json!({"a": 1}), json!([2])]);
    let refused = "the response Content-Type is not a JSON Lines media type (application/jsonl, application/jsonlines, application/x-jsonlines)";
    let downloads = MediaPolicy::standard_or(&["application/octet-stream"]);
    assert_eq!(
        open_with(MediaPolicy::standard(), Some("application/octet-stream")).await,
        Err(refused)
    );
    assert_eq!(
        open_with(downloads, Some("application/octet-stream")).await,
        records
    );
    assert_eq!(
        open_with(downloads, Some("application/jsonl; charset=utf-8")).await,
        records
    );
    assert_eq!(open_with(downloads, Some("text/plain")).await, Err(refused));
    let only = MediaPolicy::only(&["application/vnd.acme.export+jsonl"]);
    assert_eq!(
        open_with(only, Some("application/vnd.acme.export+jsonl")).await,
        records
    );
    assert_eq!(
        open_with(only, Some("application/jsonl")).await,
        Err("the response Content-Type is not one the reader's media policy names")
    );
    assert_eq!(
        open_with(
            only,
            Some("application/vnd.acme.export+jsonl; charset=latin1")
        )
        .await,
        Err("the response declares a charset other than utf-8")
    );
    assert_eq!(
        open_with(MediaPolicy::standard(), None).await,
        Err("the response has no Content-Type")
    );
    assert_eq!(open_with(MediaPolicy::any(), None).await, records);
    assert_eq!(
        open_with(MediaPolicy::any(), Some("text/html; charset=latin1")).await,
        records
    );
}

/// Errors after reading started show static labels, numbers and the operation, never the body:
/// the serde error that quotes the record is reachable only through `source_unredacted`.
#[tokio::test]
async fn read_diagnostics_never_show_body_content() {
    let body: &'static [u8] = b"{\"id\":1}\n{\"id\":\"SECRET-7f3a91\"}\n{\"token\":SECRET-7f3a91}\n\"SECRET-7f3a91\xff\"\n";
    let peer = serve(Some("application/jsonl"), body).await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let options =
        JsonLinesOptions::json_lines().record_errors(concord_extras::jsonl::RecordErrors::Yield);
    let mut records = JsonLinesReader::new(options)
        .open_for::<Rec>(response, "bulk_export")
        .unwrap();
    assert_eq!(records.next().await.unwrap().unwrap(), Rec { id: 1 });
    let mut kinds = Vec::new();
    while let Some(item) = records.next().await {
        let error = item.unwrap_err();
        let shown = format!("{error} | {error:?}");
        assert!(shown.starts_with("bulk_export: "), "{shown}");
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(!shown.contains("7f3a91"), "{shown}");
        kinds.push(error.kind());
        if error.kind() == ReadErrorKind::Decode {
            let source = error
                .source_unredacted()
                .expect("the serde error")
                .to_string();
            assert!(source.contains(SECRET), "unredacted on request: {source}");
        }
    }
    assert_eq!(
        kinds,
        [
            ReadErrorKind::Decode,
            ReadErrorKind::Format,
            ReadErrorKind::Format
        ]
    );
    drop(records);
    drop(client);
    peer.join().await;
}

/// Many records over HTTP, written by the server in pieces that cut records anywhere: all
/// arrive, in order.
#[tokio::test]
async fn many_records_over_http_in_arbitrary_pieces() {
    const N: u32 = 20_000;
    let peer = Peer::start(|mut s| async move {
        read_head(&mut s).await;
        write_head(
            &mut s,
            "200 OK",
            &[
                "content-type: application/jsonl",
                "transfer-encoding: chunked",
            ],
        )
        .await;
        let mut pending = Vec::new();
        let mut piece = 1;
        for id in 0..N {
            pending.extend_from_slice(format!("{{\"id\":{id}}}\n").as_bytes());
            piece = piece % 997 + 13;
            if pending.len() >= piece {
                write_chunk(&mut s, &pending).await;
                pending.clear();
            }
        }
        write_chunk(&mut s, &pending).await;
        end_chunks(&mut s).await;
        until_closed(&mut s).await;
    })
    .await;
    let client = client();
    let response = client.get(peer.url()).send().await.unwrap();
    let mut records = JsonLinesReader::new(JsonLinesOptions::json_lines())
        .open::<Rec>(response)
        .unwrap();
    let mut next = 0;
    while let Some(record) = records.next().await {
        assert_eq!(record.unwrap().id, next);
        next += 1;
    }
    assert_eq!(next, N);
    drop(records);
    drop(client);
    peer.join().await;
}
