//! The CSV routes against owned loopback servers (CSV.md §4 media contract, §7, §8 items 10, 13,
//! 14, 15; STREAMING.md opening, cancellation and upload rules): head refusal before any body
//! byte (with a body that never arrives), `header=` contradictions, the finite decoder's
//! `Metadata` refusals, a first record before the server finishes, a late transport failure after
//! delivered rows, safe diagnostics, a paused consumer and an early drop over real sockets, and
//! lazy uploads (zero polls at preparation, one pass at send, observed late-failure timing).

#![cfg(feature = "csv")]

mod csv_support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_core::reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderValue};
use concord_core::reqwest::{StatusCode, Version};
use concord_core::{Decode, Decoder, Encode, ErrorKind, ResponseHead};
use concord_extras::IgnoreContentType;
use concord_extras::csv::{
    Csv, CsvOptions, CsvReader, CsvWriter, HasHeader, HeaderMode, MediaPolicy, NoHeader, StreamRows,
};
use concord_extras::stream::{OpenErrorKind, ReadErrorKind, from_iter};
use csv_support::{
    Loopback, client, read_chunked_body, read_head, write_chunk, write_end, write_head,
};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Notify, oneshot};
use tokio::time::timeout;

const SECOND: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
struct Row {
    id: u32,
    name: String,
}

fn rows(n: u32) -> Vec<Row> {
    (1..=n)
        .map(|id| Row {
            id,
            name: format!("name-{id}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Head rules, on hand-built heads (no network).

fn head_verdict(reader: &CsvReader, content_types: &[&str]) -> Result<(), &'static str> {
    let mut map = HeaderMap::new();
    for ct in content_types {
        map.append(CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
    }
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    reader.check_head(&head).map_err(|e| e.label())
}

fn decode_verdict<F: Decode<Vec<Row>>>(content_types: &[&str]) -> Result<(), &'static str> {
    let mut map = HeaderMap::new();
    for ct in content_types {
        map.append(CONTENT_TYPE, HeaderValue::from_str(ct).unwrap());
    }
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &map);
    F::check_content_type(&head)
        .and_then(|()| F::context(&head).map(drop))
        .map_err(|e| e.label())
}

#[test]
fn head_rules_of_the_reader_and_the_finite_decoder_agree() {
    let has = Csv::<HasHeader>::reader().unwrap();
    let no = Csv::<NoHeader>::reader().unwrap();
    let not_csv = "the response Content-Type is not text/csv";
    type Verdict = Result<(), &'static str>;
    let cases: [(&[&str], Verdict, Verdict); 15] = [
        (&["text/csv"], Ok(()), Ok(())),
        (&["TEXT/CSV; Charset=\"UTF-8\""], Ok(()), Ok(())),
        (
            &["text/csv; charset=us-ascii; header=present"],
            Ok(()),
            Err("the CSV response declares header=present, but this format reads no header"),
        ),
        (
            &["text/csv;header=ABSENT"],
            Err("the CSV response declares header=absent, but this format expects a header"),
            Ok(()),
        ),
        (
            &["text/csv; header=maybe"],
            Err("the CSV response's header parameter is neither present nor absent"),
            Err("the CSV response's header parameter is neither present nor absent"),
        ),
        (
            &["text/csv; header=present; header=present"],
            Err("the response Content-Type declares header more than once"),
            Err("the response Content-Type declares header more than once"),
        ),
        (
            &["text/csv; charset=latin1"],
            Err("the CSV response declares a charset other than utf-8"),
            Err("the CSV response declares a charset other than utf-8"),
        ),
        (
            &["text/csv; charset=utf-8; charset=utf-8"],
            Err("the response Content-Type declares charset more than once"),
            Err("the response Content-Type declares charset more than once"),
        ),
        (
            &[],
            Err("the response has no Content-Type"),
            Err("the response has no Content-Type"),
        ),
        (
            &["text/csv", "text/csv"],
            Err("the response has more than one Content-Type field"),
            Err("the response has more than one Content-Type field"),
        ),
        (&["text/plain"], Err(not_csv), Err(not_csv)),
        (&["application/csv"], Err(not_csv), Err(not_csv)),
        (&["text/csv+json"], Err(not_csv), Err(not_csv)),
        (&["text/*"], Err(not_csv), Err(not_csv)),
        (
            &["text/csv; header"],
            Err("the response Content-Type is not a media type"),
            Err("the response Content-Type is not a media type"),
        ),
    ];
    for (cts, with_header, without) in cases {
        assert_eq!(head_verdict(&has, cts), with_header, "{cts:?}");
        assert_eq!(decode_verdict::<Csv>(cts), with_header, "{cts:?}");
        assert_eq!(head_verdict(&no, cts), without, "{cts:?}");
        assert_eq!(decode_verdict::<Csv<NoHeader>>(cts), without, "{cts:?}");
    }
    assert_eq!(<Csv as Decode<Vec<Row>>>::ACCEPT, Some("text/csv"));

    // Profile media policies.
    let missing_ok = CsvReader::new(
        CsvOptions::new(HeaderMode::Present, b',').media(MediaPolicy::TextCsvOrMissing),
    )
    .unwrap();
    assert_eq!(head_verdict(&missing_ok, &[]), Ok(()));
    assert_eq!(head_verdict(&missing_ok, &["text/plain"]), Err(not_csv));
    assert!(head_verdict(&missing_ok, &["text/csv; header=absent"]).is_err());
    let unchecked =
        CsvReader::new(CsvOptions::new(HeaderMode::Present, b',').media(MediaPolicy::Unchecked))
            .unwrap();
    for cts in [
        &[][..],
        &["application/octet-stream"],
        &["text/csv; header=absent"],
    ] {
        assert_eq!(head_verdict(&unchecked, cts), Ok(()));
    }

    // The deliberate media override skips exactly the Content-Type check (the `header=`
    // contradiction included), not the configuration check.
    assert_eq!(
        decode_verdict::<IgnoreContentType<Csv>>(&["text/csv; header=absent"]),
        Ok(())
    );
    assert_eq!(
        decode_verdict::<IgnoreContentType<Csv>>(&["text/html"]),
        Ok(())
    );
    assert_eq!(
        decode_verdict::<IgnoreContentType<Csv<HasHeader, concord_extras::csv::Delimiter<b'"'>>>>(
            &["text/csv"]
        ),
        Err("the CSV delimiter equals the quote")
    );
}

#[test]
fn the_media_override_does_not_bypass_table_rules() {
    let body = concord_core::bytes::Bytes::from_static(b"id,id\n1,2\n");
    let context = <IgnoreContentType<Csv> as Decode<Vec<Row>>>::context(&ResponseHead::new(
        StatusCode::OK,
        Version::HTTP_11,
        &HeaderMap::new(),
    ))
    .unwrap();
    let error =
        <IgnoreContentType<Csv> as Decode<Vec<Row>>>::decode_body(context, body).unwrap_err();
    assert_eq!(error.label(), "the CSV header repeats a name");
}

// ---------------------------------------------------------------------------------------------
// Opening: refusals keep the body unread; a body that never arrives does not block them.

/// A server that answers with this head, then sends `body` only when notified.
async fn held_body_server(
    status: &'static str,
    content_type: &'static str,
    body: &'static [u8],
) -> (Loopback, Arc<Notify>) {
    let release = Arc::new(Notify::new());
    let gate = Arc::clone(&release);
    let server = Loopback::start(move |mut stream| {
        let gate = Arc::clone(&gate);
        async move {
            read_head(&mut stream).await;
            let head = format!(
                "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\nx-token: s3cr3t-header\r\ntransfer-encoding: chunked\r\n\r\n"
            );
            stream.write_all(head.as_bytes()).await.unwrap();
            gate.notified().await;
            let _ = write_chunk(&mut stream, body).await;
            let _ = write_end(&mut stream).await;
            // Keep the connection until the client is done.
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    })
    .await;
    (server, release)
}

#[tokio::test]
async fn head_refusal_leaves_the_body_unread_even_when_it_never_arrives() {
    let body: &[u8] = b"id,name\n1,s3cr3t-cell\n";
    for (status, content_type, reader, kind, label) in [
        (
            "200 OK",
            "text/plain",
            Csv::<HasHeader>::reader().unwrap(),
            OpenErrorKind::Metadata,
            Some("the response Content-Type is not text/csv"),
        ),
        (
            "200 OK",
            "text/csv; header=absent",
            Csv::<HasHeader>::reader().unwrap(),
            OpenErrorKind::Metadata,
            Some("the CSV response declares header=absent, but this format expects a header"),
        ),
        (
            "200 OK",
            "text/csv; header=present",
            Csv::<NoHeader>::reader().unwrap(),
            OpenErrorKind::Metadata,
            Some("the CSV response declares header=present, but this format reads no header"),
        ),
        (
            "503 Service Unavailable",
            "text/csv",
            Csv::<HasHeader>::reader().unwrap(),
            OpenErrorKind::Status,
            None,
        ),
    ] {
        let (server, release) = held_body_server(status, content_type, body).await;
        let response = timeout(
            SECOND,
            client()
                .get(format!("{}/export/s3cr3t-path", server.url))
                .send(),
        )
        .await
        .unwrap()
        .unwrap();
        // The body has not been sent: opening must not wait for it.
        let error = reader.open_for::<Row>(response, "test export").unwrap_err();
        assert_eq!(error.kind(), kind);
        assert_eq!(error.codec().map(|c| c.label()), label);
        assert_eq!(error.operation(), Some("test export"));
        let shown = format!("{error} {error:?}");
        for secret in ["s3cr3t", "127.0.0.1", "export/"] {
            assert!(!shown.contains(secret), "{secret} in {shown}");
        }
        // Unread: the whole body is still there once the server sends it.
        let response = error.into_response();
        release.notify_one();
        let text = timeout(SECOND, response.bytes()).await.unwrap().unwrap();
        assert_eq!(text, body);
        server.stop().await;
    }
}

#[tokio::test]
async fn the_finite_decoder_refuses_the_head_before_reading_the_body() {
    let (server, release) =
        held_body_server("200 OK", "text/csv; header=absent", b"id,name\n1,a\n").await;
    let response = client().get(&server.url).send().await.unwrap();
    let error = timeout(SECOND, Decoder::<Vec<Row>>::new::<Csv>().decode(response))
        .await
        .expect("no body read")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(
        error.codec_error().unwrap().label(),
        "the CSV response declares header=absent, but this format expects a header"
    );
    let response = error.into_response().unwrap();
    release.notify_one();
    assert_eq!(response.bytes().await.unwrap(), &b"id,name\n1,a\n"[..]);
    server.stop().await;

    // And a matching head decodes.
    let (server, release) = held_body_server(
        "200 OK",
        "text/csv; header=present; charset=utf-8",
        b"name,id\nx,7\n",
    )
    .await;
    release.notify_one();
    let response = client().get(&server.url).send().await.unwrap();
    let rows = Decoder::<Vec<Row>>::new::<Csv>()
        .decode(response)
        .await
        .unwrap();
    assert_eq!(
        rows,
        [Row {
            id: 7,
            name: "x".into()
        }]
    );
    server.stop().await;
}

#[tokio::test]
async fn an_open_succeeds_immediately_and_the_first_record_arrives_before_the_body_ends() {
    let (finish_tx, finish_rx) = oneshot::channel::<()>();
    let finish = Arc::new(Mutex::new(Some(finish_rx)));
    let server = Loopback::start(move |mut stream| {
        let finish = finish.lock().unwrap().take();
        async move {
            read_head(&mut stream).await;
            write_head(&mut stream, &[("content-type", "text/csv; header=present")])
                .await
                .unwrap();
            write_chunk(&mut stream, b"id,name\n1,first\n")
                .await
                .unwrap();
            // The rest of the body waits for the test.
            if let Some(finish) = finish {
                let _ = finish.await;
            }
            let _ = write_chunk(&mut stream, b"2,second\n").await;
            let _ = write_end(&mut stream).await;
        }
    })
    .await;
    let response = client().get(&server.url).send().await.unwrap();
    let mut records = Csv::<HasHeader>::reader()
        .unwrap()
        .open::<Row>(response)
        .unwrap();
    let first = timeout(SECOND, records.next())
        .await
        .expect("first record before the body ends");
    assert_eq!(
        first.unwrap().unwrap(),
        Row {
            id: 1,
            name: "first".into()
        }
    );
    assert_eq!(
        records.headers().unwrap().to_strings().unwrap(),
        ["id", "name"]
    );
    finish_tx.send(()).unwrap();
    assert_eq!(records.next().await.unwrap().unwrap().id, 2);
    assert!(records.next().await.is_none());
    server.stop().await;
}

#[tokio::test]
async fn a_late_transport_failure_keeps_the_delivered_prefix_and_fuses() {
    let server = Loopback::start(|mut stream| async move {
        read_head(&mut stream).await;
        write_head(&mut stream, &[("content-type", "text/csv")])
            .await
            .unwrap();
        write_chunk(&mut stream, b"id,name\n1,a\n2,b\n3,s3cr3t-unfinish")
            .await
            .unwrap();
        // Abort: no last chunk.
        drop(stream);
    })
    .await;
    let response = client()
        .get(format!("{}/s3cr3t-path", server.url))
        .send()
        .await
        .unwrap();
    let mut records = Csv::<HasHeader>::reader()
        .unwrap()
        .open_for::<Row>(response, "export")
        .unwrap();
    assert_eq!(records.next().await.unwrap().unwrap().id, 1);
    assert_eq!(records.next().await.unwrap().unwrap().id, 2);
    let error = timeout(SECOND, records.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Transport);
    assert_eq!(error.position().records, 2);
    assert_eq!(error.operation(), Some("export"));
    let shown = format!("{error} {error:?} {records:?}");
    for secret in ["s3cr3t", "127.0.0.1", "http"] {
        assert!(!shown.contains(secret), "{secret} in {shown}");
    }
    assert!(
        error.to_string().starts_with(
            "export: reading failed: reading the response body failed (after 2 records"
        )
    );
    // The underlying reqwest error is reachable deliberately, without its URL.
    let source = error.source_unredacted().unwrap().to_string();
    assert!(!source.contains("s3cr3t-path"), "{source}");
    assert!(records.next().await.is_none());
    server.stop().await;
}

#[tokio::test]
async fn decode_and_format_errors_do_not_show_cells_or_headers() {
    let server = Loopback::start(|mut stream| async move {
        read_head(&mut stream).await;
        write_head(
            &mut stream,
            &[
                ("content-type", "text/csv"),
                ("set-cookie", "session=s3cr3t-cookie"),
            ],
        )
        .await
        .unwrap();
        write_chunk(&mut stream, b"id,name\nnot-a-number-s3cr3t-cell,x\n")
            .await
            .unwrap();
        write_end(&mut stream).await.unwrap();
    })
    .await;
    let response = client().get(&server.url).send().await.unwrap();
    let mut records = Csv::<HasHeader>::reader()
        .unwrap()
        .open::<Row>(response)
        .unwrap();
    let error = records.next().await.unwrap().unwrap_err();
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    let shown = format!("{error} {error:?} {records:?} {:?}", records.headers());
    assert!(!shown.contains("s3cr3t"), "{shown}");
    server.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Back-pressure and cancellation over a real socket.

/// A server that writes rows until the client stops reading or goes away; reports bytes written
/// and whether a write failed (the connection was released).
async fn firehose() -> (Loopback, Arc<AtomicUsize>, Arc<Notify>) {
    let written = Arc::new(AtomicUsize::new(0));
    let closed = Arc::new(Notify::new());
    let (w, c) = (Arc::clone(&written), Arc::clone(&closed));
    let server = Loopback::start(move |mut stream| {
        let (written, closed) = (Arc::clone(&w), Arc::clone(&c));
        async move {
            read_head(&mut stream).await;
            write_head(&mut stream, &[("content-type", "text/csv")])
                .await
                .unwrap();
            let _ = write_chunk(&mut stream, b"id,name\n").await;
            let row = b"12345,abcdefghijklmnopqrstuvwxyz0123456789\n".repeat(1500); // ~64 KiB
            let limit = 512 * 1024 * 1024;
            while written.load(Ordering::SeqCst) < limit {
                if write_chunk(&mut stream, &row).await.is_err() {
                    break;
                }
                written.fetch_add(row.len(), Ordering::SeqCst);
            }
            closed.notify_one();
        }
    })
    .await;
    (server, written, closed)
}

#[tokio::test]
async fn a_paused_consumer_stalls_the_server_and_a_drop_releases_the_connection() {
    let (server, written, closed) = firehose().await;
    let response = client().get(&server.url).send().await.unwrap();
    let mut records = Csv::<HasHeader>::reader()
        .unwrap()
        .open::<(u32, String)>(response)
        .unwrap();
    assert_eq!(records.next().await.unwrap().unwrap().0, 12345);

    // The consumer pauses: the server can only fill the socket and transport buffers.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after_pause = written.load(Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after_second_pause = written.load(Ordering::SeqCst);
    println!(
        "paused consumer: server wrote {after_pause} bytes, then {after_second_pause} (of 512 MiB); reader retains {} bytes",
        records.retained_bytes()
    );
    assert_eq!(after_pause, after_second_pause, "the producer is stalled");
    assert!(after_pause < 64 * 1024 * 1024, "{after_pause}");
    assert!(records.retained_bytes() <= 256 + 16 * 8);

    // Dropping the reader drops the response: the server's next writes fail.
    drop(records);
    timeout(Duration::from_secs(5), closed.notified())
        .await
        .expect("the connection is released after the drop");
    server.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Lazy uploads.

/// A server that reads one chunked request body and reports what arrived (data, whether the
/// last chunk arrived, the request head), answering 200 only to a complete body.
async fn upload_sink() -> (Loopback, Arc<Mutex<Vec<(String, Vec<u8>, bool)>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s = Arc::clone(&seen);
    let server = Loopback::start(move |mut stream| {
        let seen = Arc::clone(&s);
        async move {
            let head = read_head(&mut stream).await;
            let (body, complete) = read_chunked_body(&mut stream).await;
            seen.lock().unwrap().push((head, body, complete));
            if complete {
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        }
    })
    .await;
    (server, seen)
}

/// A one-shot source of `items` whose polls are counted.
fn counted<T: Send + Unpin + 'static>(
    items: Vec<Result<T, std::io::Error>>,
) -> (
    impl futures_core::Stream<Item = Result<T, std::io::Error>> + Send + 'static,
    Arc<AtomicUsize>,
) {
    let polls = Arc::new(AtomicUsize::new(0));
    let p = Arc::clone(&polls);
    let mut items = items.into_iter();
    let source = from_iter(std::iter::from_fn(move || {
        p.fetch_add(1, Ordering::SeqCst);
        items.next()
    }));
    (source, polls)
}

#[tokio::test]
async fn an_upload_polls_its_source_zero_times_until_sent_then_once_through() {
    let (server, seen) = upload_sink().await;
    let (source, polls) = counted(rows(3).into_iter().map(Ok).collect());
    let body = <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(source)).unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0, "preparation polls nothing");
    let response = client()
        .post(&server.url)
        .header(CONTENT_TYPE, Csv::<HasHeader>::CONTENT_TYPE)
        .body(body)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    // Three rows and the end: four polls, one pass.
    assert_eq!(polls.load(Ordering::SeqCst), 4);
    let (head, data, complete) = seen.lock().unwrap().pop().unwrap();
    assert!(complete);
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: text/csv; header=present"),
        "{head}"
    );
    assert!(
        head.to_ascii_lowercase()
            .contains("transfer-encoding: chunked"),
        "{head}"
    );
    assert_eq!(data, b"id,name\r\n1,name-1\r\n2,name-2\r\n3,name-3\r\n");
    server.stop().await;
}

#[tokio::test]
async fn a_runtime_upload_carries_its_matching_content_type() {
    let (server, seen) = upload_sink().await;
    let writer = CsvWriter::new(CsvOptions::new(HeaderMode::Absent, b';')).unwrap();
    let (source, polls) = counted(vec![Ok((1, "a;b")), Ok((2, "c"))]);
    let upload = writer.upload(StreamRows::new(source)).unwrap();
    assert_eq!(upload.content_type(), "text/csv; header=absent");
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let http = client();
    let mut request = http
        .post(&server.url)
        .header(CONTENT_TYPE, "application/json")
        .build()
        .unwrap();
    upload.apply(&mut request);
    let response = http.execute(request).await.unwrap();
    assert_eq!(response.status(), 200);
    let (head, data, _) = seen.lock().unwrap().pop().unwrap();
    assert!(
        head.to_ascii_lowercase()
            .contains("content-type: text/csv; header=absent"),
        "{head}"
    );
    assert!(!head.contains("application/json"), "{head}");
    assert_eq!(data, b"1;\"a;b\"\r\n2;c\r\n");
    server.stop().await;
}

/// Sends `body` and returns the client's error and what the server received.
async fn send_failing(
    body: concord_core::reqwest::Body,
) -> (concord_core::reqwest::Error, Vec<u8>, bool) {
    let (server, seen) = upload_sink().await;
    let error = timeout(
        Duration::from_secs(5),
        client().post(&server.url).body(body).send(),
    )
    .await
    .expect("the send ends")
    .unwrap_err();
    let (_, data, complete) = wait_for_record(&seen).await;
    server.stop().await;
    (error, data, complete)
}

#[tokio::test]
async fn late_upload_failures_happen_after_transmission_started() {
    // 1. A header-required source that ends empty without columns: preparation succeeds (nothing
    // is polled), the send fails, the server saw the request head and no complete body.
    let (source, polls) = counted(Vec::<Result<Row, _>>::new());
    let body = <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(source)).unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let (error, data, complete) = send_failing(body).await;
    assert!(error.is_request() || error.is_body(), "{error:?}");
    assert!(!complete);
    assert!(data.is_empty());
    let upload_error = std::error::Error::source(&error)
        .and_then(|e| find_upload_error(e))
        .expect("the upload error is in the chain");
    assert_eq!(
        upload_error.codec().unwrap().label(),
        "an empty CSV table with a header needs explicit columns"
    );

    // 2. An always-ready source whose third item fails. Observed: hyper keeps polling the ready
    // body into its write queue and aborts the connection on the error before flushing it, so
    // the server may see the head and no body byte. The failure is still after the request was
    // sent (a body error of the transfer, not a pre-send `Build`). Paced failures, with the
    // server holding a prefix first, are in the next test.
    let items: Vec<Result<Row, std::io::Error>> = vec![
        Ok(Row {
            id: 1,
            name: "a".into(),
        }),
        Ok(Row {
            id: 2,
            name: "b".into(),
        }),
        Err(std::io::Error::other("synthetic")),
    ];
    let (source, polls) = counted(items);
    let body = <Csv as Encode<StreamRows<_>>>::encode(StreamRows::new(source)).unwrap();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let (error, data, complete) = send_failing(body).await;
    assert!(!complete);
    assert!(error.is_request() || error.is_body(), "{error:?}");
    println!(
        "always-ready source failing at item 3: the server received {} body bytes before the abort",
        data.len()
    );
}

async fn wait_for_record(seen: &Mutex<Vec<(String, Vec<u8>, bool)>>) -> (String, Vec<u8>, bool) {
    for _ in 0..500 {
        if let Some(record) = seen.lock().unwrap().pop() {
            return record;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the server saw no request");
}

/// A one-shot source fed by the test, row by row: pending between rows, like a source that
/// waits on its own I/O.
struct Paced<T>(tokio::sync::mpsc::UnboundedReceiver<Result<T, std::io::Error>>);

impl<T> futures_core::Stream for Paced<T> {
    type Item = Result<T, std::io::Error>;
    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.0.poll_recv(cx)
    }
}

/// A server that records the request body bytes as they arrive, and whether the body completed.
async fn progress_sink() -> (Loopback, Arc<Mutex<Vec<u8>>>, Arc<Mutex<Option<bool>>>) {
    let received = Arc::new(Mutex::new(Vec::new()));
    let complete = Arc::new(Mutex::new(None));
    let (r, c) = (Arc::clone(&received), Arc::clone(&complete));
    let server = Loopback::start(move |mut stream| {
        let (received, complete) = (Arc::clone(&r), Arc::clone(&c));
        async move {
            use tokio::io::AsyncReadExt;
            read_head(&mut stream).await;
            let mut raw = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                match stream.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        raw.extend_from_slice(&buf[..n]);
                        *received.lock().unwrap() = csv_support::chunked_data(&raw);
                        if raw.ends_with(b"0\r\n\r\n") {
                            *complete.lock().unwrap() = Some(true);
                            let _ = stream
                                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                                .await;
                            return;
                        }
                    }
                }
            }
            *complete.lock().unwrap() = Some(false);
        }
    })
    .await;
    (server, received, complete)
}

/// Sends `before` rows paced by the test, waits until the server has received `prefix`, then
/// feeds `fail`. Returns the client's error and the server's final view (data, completed).
async fn paced_failure<T, F>(
    encode: F,
    before: Vec<T>,
    prefix: &[u8],
    fail: Result<T, std::io::Error>,
) -> (concord_core::reqwest::Error, Vec<u8>, Option<bool>)
where
    T: Send + 'static,
    F: FnOnce(StreamRows<Paced<T>>) -> concord_core::reqwest::Body,
{
    let (server, received, complete) = progress_sink().await;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let body = encode(StreamRows::new(Paced(rx)));
    let url = server.url.clone();
    let send = tokio::spawn(async move { client().post(url).body(body).send().await });
    for row in before {
        tx.send(Ok(row)).unwrap();
    }
    let mut waited = 0;
    while received.lock().unwrap().as_slice() != prefix {
        tokio::time::sleep(Duration::from_millis(5)).await;
        waited += 1;
        assert!(waited < 1000, "the server never received the prefix");
    }
    // Transmission has started: the server holds the prefix. Now the failure.
    tx.send(fail).unwrap();
    let error = timeout(Duration::from_secs(5), send)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    let mut waited = 0;
    while complete.lock().unwrap().is_none() && waited < 500 {
        tokio::time::sleep(Duration::from_millis(5)).await;
        waited += 1;
    }
    let data = received.lock().unwrap().clone();
    let complete = *complete.lock().unwrap();
    server.stop().await;
    (error, data, complete)
}

#[tokio::test]
async fn paced_late_failures_after_the_server_received_rows() {
    // A row that cannot be serialized.
    #[derive(serde::Serialize)]
    struct Flaky {
        #[serde(serialize_with = "fail_on_zero")]
        id: u32,
        name: &'static str,
    }
    fn fail_on_zero<S: serde::Serializer>(v: &u32, s: S) -> Result<S::Ok, S::Error> {
        if *v == 0 {
            Err(serde::ser::Error::custom("synthetic s3cr3t failure"))
        } else {
            s.serialize_u32(*v)
        }
    }
    let prefix = b"id,name\r\n1,a\r\n2,b\r\n";
    let (error, data, complete) = paced_failure(
        |rows| <Csv as Encode<StreamRows<_>>>::encode(rows).unwrap(),
        vec![Flaky { id: 1, name: "a" }, Flaky { id: 2, name: "b" }],
        prefix,
        Ok(Flaky {
            id: 0,
            name: "s3cr3t-cell",
        }),
    )
    .await;
    assert_eq!(complete, Some(false), "the body never completes");
    assert_eq!(data, prefix, "nothing after the prefix");
    let upload = std::error::Error::source(&error)
        .and_then(find_upload_error)
        .unwrap();
    assert_eq!(upload.records(), 2);
    assert_eq!(
        upload.codec().unwrap().label(),
        "a CSV row cannot be written"
    );
    assert_eq!(upload.codec().unwrap().get("record"), Some(3));
    let shown = format!("{error} {error:?} {upload} {upload:?}");
    assert!(!shown.contains("s3cr3t"), "{shown}");

    // A source error.
    let prefix = b"id,name\r\n1,name-1\r\n2,name-2\r\n";
    let (error, data, complete) = paced_failure(
        |rows| <Csv as Encode<StreamRows<_>>>::encode(rows).unwrap(),
        rows(2),
        prefix,
        Err(std::io::Error::other("source s3cr3t gone")),
    )
    .await;
    assert_eq!(complete, Some(false));
    assert_eq!(data, prefix);
    let upload = std::error::Error::source(&error)
        .and_then(find_upload_error)
        .unwrap();
    assert_eq!(upload.label(), "the upload source failed");
    assert!(!format!("{upload} {upload:?}").contains("s3cr3t"));
    assert!(
        upload
            .source_unredacted()
            .unwrap()
            .to_string()
            .contains("s3cr3t")
    );

    // A same-width change of field names.
    struct Conditional(bool);
    impl serde::Serialize for Conditional {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            use serde::ser::SerializeStruct;
            let mut st = s.serialize_struct("C", 1)?;
            if self.0 {
                st.serialize_field("approved", &1)?;
            } else {
                st.serialize_field("rejected", &2)?;
            }
            st.end()
        }
    }
    let (error, data, complete) = paced_failure(
        |rows| <Csv as Encode<StreamRows<_>>>::encode(rows).unwrap(),
        vec![Conditional(true)],
        b"approved\r\n1\r\n",
        Ok(Conditional(false)),
    )
    .await;
    assert_eq!(complete, Some(false));
    assert_eq!(data, b"approved\r\n1\r\n");
    let upload = std::error::Error::source(&error)
        .and_then(find_upload_error)
        .unwrap();
    assert_eq!(
        upload.codec().unwrap().label(),
        "a CSV row changes the field names of the table"
    );
}

/// The `UploadError` in an error chain.
fn find_upload_error<'a>(
    mut error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a concord_extras::stream::UploadError> {
    loop {
        if let Some(found) = error.downcast_ref::<concord_extras::stream::UploadError>() {
            return Some(found);
        }
        error = error.source()?;
    }
}
