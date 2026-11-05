use concord::{
    csv::{Csv, CsvOptions, CsvReader, HeaderMode, StreamRows},
    reqwest,
};
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    convert::Infallible,
    marker::PhantomPinned,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

#[derive(Serialize, Deserialize, Debug)]
pub struct Row {
    id: usize,
    value: String,
}

// Deliberately Send + !Sync + !Unpin, to test the published StreamRows bounds too.
pub struct Source {
    polls: Arc<AtomicUsize>,
    emitted: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
    next: Cell<usize>,
    delay: RefCell<Pin<Box<tokio::time::Sleep>>>,
    _pin: PhantomPinned,
}
impl Stream for Source {
    type Item = Result<Row, Infallible>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.as_ref().get_ref();
        this.polls.fetch_add(1, SeqCst);
        if this.delay.borrow_mut().as_mut().poll(cx).is_pending() {
            return Poll::Pending;
        }
        let id = this.next.get();
        if id == 100_000 {
            return Poll::Ready(None);
        }
        this.next.set(id + 1);
        this.emitted.fetch_add(1, SeqCst);
        this.delay
            .borrow_mut()
            .as_mut()
            .reset(tokio::time::Instant::now() + Duration::from_millis(20));
        Poll::Ready(Some(Ok(Row {
            id,
            value: "x".repeat(8192),
        })))
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.drops.fetch_add(1, SeqCst);
    }
}

concord::api! {
    pub client CsvApi {
        base "http://127.0.0.1";
        POST upload(body rows: StreamRows<Source>) { request Csv; path "upload"; }
        GET export() -> Vec<Row> { response Csv; path "export"; }
    }
}

async fn head(socket: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        bytes.push(socket.read_u8().await.unwrap());
        assert!(bytes.len() < 32_768);
    }
    bytes
}

#[tokio::test]
async fn provider_typed_value_keeps_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = head(&mut socket).await;
        assert!(String::from_utf8_lossy(&request).starts_with("GET /repos/acme/demo/labels "));
        let body = r#"[{"id":7,"name":"bug","color":"ff0000","description":null,"default":false}]"#;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nX-GitHub-Request-Id: astra-7\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
    });
    let api = github::GitHub::new(
        reqwest::Client::new(),
        github::GitHubConfig {
            token: "test-only".into(),
        },
    )
    .with_base_url(base)
    .unwrap();
    let prepared = api
        .repo(
            github::ids::Owner::new("acme"),
            github::ids::RepoName::new("demo"),
        )
        .labels()
        .prepare()
        .unwrap();
    let (status, request_id, labels) = astra_review_task::with_metadata(prepared).await.unwrap();
    assert_eq!(status, reqwest::StatusCode::OK);
    assert_eq!(request_id.unwrap(), "astra-7");
    assert_eq!(labels[0].name, "bug");
    println!(
        "metadata: status={status}, x-github-request-id=astra-7, typed label={}",
        labels[0].name
    );
    server.await.unwrap();
}

#[tokio::test]
async fn declared_stream_upload_prepares_without_polling_and_cancels() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (seen_tx, seen_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = String::from_utf8(head(&mut socket).await)
            .unwrap()
            .to_lowercase();
        assert!(request.contains("content-type: text/csv; header=present"));
        assert!(request.contains("transfer-encoding: chunked"));
        let mut seen_tx = Some(seen_tx);
        let mut received = Vec::new();
        let mut chunk = [0; 1024];
        let ended = tokio::time::timeout(Duration::from_secs(4), async {
            loop {
                match socket.read(&mut chunk).await {
                    Ok(0) => break "eof",
                    Ok(n) => {
                        received.extend_from_slice(&chunk[..n]);
                        if let Some(tx) = seen_tx.take() {
                            let _ = tx.send(());
                        }
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    Err(_) => break "reset",
                }
            }
        })
        .await
        .unwrap();
        assert!(
            !received.ends_with(b"0\r\n\r\n"),
            "cancelled request must lack the final chunk"
        );
        assert!(received.windows(10).any(|w| w == b"id,value\r\n"));
        (received.len(), ended)
    });
    let polls = Arc::new(AtomicUsize::new(0));
    let emitted = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let source = Source {
        polls: polls.clone(),
        emitted: emitted.clone(),
        drops: drops.clone(),
        next: Cell::new(0),
        delay: RefCell::new(Box::pin(tokio::time::sleep(Duration::ZERO))),
        _pin: PhantomPinned,
    };
    let prepared = CsvApi::new(reqwest::Client::new())
        .with_base_url(base)
        .unwrap()
        .upload(StreamRows::new(source))
        .prepare()
        .unwrap();
    assert_eq!(polls.load(SeqCst), 0);
    assert_eq!(emitted.load(SeqCst), 0);
    assert!(prepared.request().try_clone().is_none());
    let send = tokio::spawn(async move { prepared.send().await });
    tokio::time::timeout(Duration::from_secs(2), seen_rx)
        .await
        .unwrap()
        .unwrap();
    tokio::time::sleep(Duration::from_millis(70)).await;
    send.abort();
    assert!(send.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), async {
        while drops.load(SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let after = polls.load(SeqCst);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert_eq!(polls.load(SeqCst), after);
    assert_eq!(drops.load(SeqCst), 1);
    assert!(emitted.load(SeqCst) > 0 && emitted.load(SeqCst) < 100_000);
    let (bytes, ended) = server.await.unwrap();
    println!(
        "upload: prepare polls=0; cancelled polls={after}; rows={}; drops=1; server wire body bytes={bytes}; terminal={ended}; final chunk absent",
        emitted.load(SeqCst)
    );
}

#[tokio::test]
async fn contradictory_runtime_header_preserves_unread_response() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (release_tx, release_rx) = oneshot::channel();
    let body = "id,value\r\n1,secret-cell\r\n";
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        head(&mut socket).await;
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/csv; header=present\r\nX-Request-Id: mismatch\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes()).await.unwrap();
        release_rx.await.unwrap();
        socket.write_all(body.as_bytes()).await.unwrap();
    });
    let response = CsvApi::new(reqwest::Client::new())
        .with_base_url(base)
        .unwrap()
        .export()
        .prepare()
        .unwrap()
        .send_raw()
        .await
        .unwrap();
    let reader = CsvReader::new(CsvOptions::new(HeaderMode::Absent, b',')).unwrap();
    let error = match reader.open::<Row>(response) {
        Ok(_) => panic!("expected contradiction refusal"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), concord::stream::OpenErrorKind::Metadata);
    assert_eq!(error.status_code(), reqwest::StatusCode::OK);
    assert_eq!(error.response().headers()["x-request-id"], "mismatch");
    assert!(!format!("{error:?} {error}").contains("secret-cell"));
    println!(
        "runtime contradiction: kind={:?}, status={}, body still withheld; {error}",
        error.kind(),
        error.status_code()
    );
    release_tx.send(()).unwrap();
    assert_eq!(error.into_response().text().await.unwrap(), body);
    server.await.unwrap();
}
