//! W1 (S02–S04): a non-`Clone` one-shot source moved into a streaming request body by a custom
//! `Encode`, against the loopback fixture (which decodes `Transfer-Encoding: chunked` bodies).

mod support;

use concord::bytes::Bytes;
use concord_example_native::upload::{OneShot, Streamed, Tally, UploadReceipt, upload_attachment};
use support::{Reply, Server, inventory_at};

/// Answers every upload with the number of body bytes received.
async fn receiver() -> Server {
    Server::start(|request| Reply::json(200, &format!(r#"{{"bytes":{}}}"#, request.body.len())))
        .await
}

fn chunks(parts: &[&'static str]) -> Vec<Bytes> {
    parts
        .iter()
        .map(|p| Bytes::from_static(p.as_bytes()))
        .collect()
}

fn untouched(tally: &Tally) {
    assert_eq!(
        (tally.polls(), tally.chunks(), tally.bytes(), tally.ended()),
        (0, 0, 0, false),
        "the source was read before the request was executed"
    );
}

/// S02: building the operation, its `Debug`, `prepare()`, `Prepared`'s `Debug` and the native
/// request's inspection never poll the source. The stream object is created by the encoder
/// (during `prepare`), which only wraps it.
#[tokio::test]
async fn preparing_and_inspecting_never_polls_the_source() {
    let server = receiver().await;
    let api = inventory_at(server.addr, "Bearer t");
    let (source, tally) = OneShot::new(chunks(&["alpha-", "beta-", "gamma"]));

    let operation = api.upload(7, source);
    untouched(&tally);
    let shown = format!("{operation:?}");
    assert!(shown.contains(r#"source: "..""#), "{shown}");
    untouched(&tally);

    let prepared = operation.prepare().unwrap();
    untouched(&tally);
    let shown = format!("{prepared:?}");
    assert!(shown.contains(r#"body: Some("...")"#), "{shown}");
    untouched(&tally);

    let request = prepared.request();
    assert_eq!(
        request.headers()["content-type"],
        "application/octet-stream"
    );
    assert!(request.headers().get("content-length").is_none());
    let body = request.body().expect("a body");
    assert!(body.as_bytes().is_none(), "a stream has no buffered bytes");
    // A streaming request cannot be copied, so nothing can replay it (no retry, no redirect
    // resend) without a fresh source.
    assert!(request.try_clone().is_none());
    let _ = format!("{request:?}");
    untouched(&tally);

    drop(prepared); // never sent: the source is dropped unread
    untouched(&tally);
    assert!(server.requests().is_empty());
}

/// S03: one `send()` is one request carrying exactly the source's bytes, chunked, with the
/// format's fixed media type; the source is drained once. A second upload takes a fresh source.
#[tokio::test]
async fn one_send_streams_the_source_once_and_a_second_upload_needs_a_fresh_source() {
    let server = receiver().await;
    let api = inventory_at(server.addr, "Bearer t");
    let parts = ["alpha-", "beta-", "gamma"];
    let total: usize = parts.iter().map(|p| p.len()).sum();
    let (source, tally) = OneShot::new(chunks(&parts));

    let receipt = upload_attachment(&api, 7, source).await.unwrap();
    assert_eq!(
        receipt,
        UploadReceipt {
            bytes: total as u64
        }
    );

    let recorded = server.single();
    assert_eq!(recorded.method, "PUT");
    assert_eq!(recorded.target, "/v1/items/7/upload");
    assert_eq!(recorded.body, b"alpha-beta-gamma");
    assert!(recorded.chunked, "a body of unknown length is sent chunked");
    assert_eq!(recorded.header("content-length"), None);
    assert_eq!(
        recorded.header("content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(recorded.chunk_sizes.iter().sum::<usize>(), total);
    eprintln!(
        "S03 wire: chunked={} chunk_sizes={:?} body={:?}; source: polls={} chunks={} bytes={} ended={}",
        recorded.chunked,
        recorded.chunk_sizes,
        String::from_utf8_lossy(&recorded.body),
        tally.polls(),
        tally.chunks(),
        tally.bytes(),
        tally.ended()
    );

    // Drained exactly once: every chunk and byte handed out once, and the end reported. (The
    // number of polls is the scheduler's business; it is at least chunks + 1.)
    assert_eq!(tally.chunks(), parts.len());
    assert_eq!(tally.bytes(), total as u64);
    assert!(tally.ended());
    assert!(tally.polls() > parts.len());

    // A second upload: the first source was moved into the first request; a fresh one is needed.
    let (fresh, fresh_tally) = OneShot::new(chunks(&["second"]));
    let receipt = upload_attachment(&api, 8, fresh).await.unwrap();
    assert_eq!(receipt, UploadReceipt { bytes: 6 });
    let all = server.requests();
    assert_eq!(all.len(), 2, "one request per upload, no hidden replay");
    assert_eq!(all[1].target, "/v1/items/8/upload");
    assert_eq!(all[1].body, b"second");
    assert_eq!((fresh_tally.chunks(), fresh_tally.bytes()), (1, 6));
    // The first source was not consumed again.
    assert_eq!((tally.chunks(), tally.bytes()), (parts.len(), total as u64));
}

/// An empty source is still one chunked request with an empty body.
#[tokio::test]
async fn an_empty_source_sends_an_empty_body() {
    let server = receiver().await;
    let api = inventory_at(server.addr, "Bearer t");
    let (source, tally) = OneShot::new(Vec::new());
    let receipt = upload_attachment(&api, 1, source).await.unwrap();
    assert_eq!(receipt.bytes, 0);
    let recorded = server.single();
    assert!(recorded.chunked, "an empty source is still sent chunked");
    assert!(recorded.body.is_empty());
    assert!(tally.ended());
    assert_eq!((tally.chunks(), tally.bytes()), (0, 0));
}

concord::api! {
    /// The same streamed format under two explicit media types (author guide 5.3 and 5.8).
    pub client Feed {
        base "http://feed.invalid";
        request Streamed;

        /// A declared media type replaces `Streamed::CONTENT_TYPE`.
        PUT ndjson(body source: OneShot) { path "ndjson"; request Streamed "application/x-ndjson"; }

        /// A `content-type` header reaching the endpoint replaces it too (sent once).
        PUT labelled(header("content-type") media: String, body source: OneShot) { path "labelled"; }
    }
}

/// An explicit media type on the endpoint wins over the format's static `CONTENT_TYPE`, for this
/// streamed body too: a declared `request Streamed "type/subtype";`, or a runtime
/// `content-type` header (the route for a value only known with the body).
#[tokio::test]
async fn an_explicit_endpoint_media_type_replaces_the_static_one() {
    let server = Server::reply(Reply::new(204)).await;
    let feed = Feed::new(concord::reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap();

    let (source, tally) = OneShot::new(chunks(&["{\"a\":1}\n", "{\"a\":2}\n"]));
    let prepared = feed.ndjson(source).prepare().unwrap();
    assert_eq!(
        prepared.request().headers()["content-type"],
        "application/x-ndjson"
    );
    untouched(&tally);
    prepared.send().await.unwrap();

    let (source, _) = OneShot::new(chunks(&["raw"]));
    feed.labelled("application/vnd.example.blob; v=2", source)
        .send()
        .await
        .unwrap();

    let all = server.requests();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].header("content-type"), Some("application/x-ndjson"));
    assert_eq!(all[0].body, b"{\"a\":1}\n{\"a\":2}\n");
    assert!(all[0].chunked);
    assert_eq!(
        all[1].header_all("content-type"),
        ["application/vnd.example.blob; v=2"]
    );
    assert_eq!(all[1].body, b"raw");
    assert_eq!(tally.bytes(), 16);
}
