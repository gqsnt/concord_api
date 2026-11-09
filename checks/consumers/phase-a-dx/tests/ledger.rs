use concord::{
    ErrorKind, reqwest,
    stream::{self, Completion},
};
use concord_pagination::{PagesError, Resume};
use consumer_phase_a_dx::*;
use std::{
    convert::Infallible,
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

#[derive(Debug)]
struct Request {
    head: String,
    body: Vec<u8>,
}
struct Reply {
    status: &'static str,
    content_type: &'static str,
    body: &'static str,
    declared_length: Option<usize>,
}
fn reply(status: &'static str, content_type: &'static str, body: &'static str) -> Reply {
    Reply {
        status,
        content_type,
        body,
        declared_length: None,
    }
}
async fn line(s: &mut TcpStream) -> Option<Vec<u8>> {
    let mut v = Vec::new();
    loop {
        let mut b = [0];
        if s.read_exact(&mut b).await.is_err() {
            return None;
        }
        v.push(b[0]);
        if v.ends_with(b"\r\n") {
            return Some(v);
        }
    }
}
async fn request(s: &mut TcpStream) -> Option<Request> {
    let mut head = Vec::new();
    loop {
        head.extend(line(s).await?);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    let head = String::from_utf8(head).unwrap();
    let lower = head.to_ascii_lowercase();
    let mut body = Vec::new();
    if lower.contains("transfer-encoding: chunked") {
        loop {
            let size_line = line(s).await?;
            let size = usize::from_str_radix(
                std::str::from_utf8(&size_line)
                    .ok()?
                    .trim()
                    .split(';')
                    .next()?,
                16,
            )
            .ok()?;
            if size == 0 {
                let _ = line(s).await;
                break;
            }
            let start = body.len();
            body.resize(start + size, 0);
            s.read_exact(&mut body[start..]).await.ok()?;
            line(s).await?;
        }
    } else if let Some(len) = lower.lines().find_map(|l| {
        l.strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    }) {
        body.resize(len, 0);
        s.read_exact(&mut body).await.ok()?;
    }
    Some(Request { head, body })
}
async fn server(
    replies: Vec<Reply>,
) -> (Ledger, mpsc::Receiver<Request>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel(20);
    let task = tokio::spawn(async move {
        for r in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            if let Some(req) = request(&mut socket).await {
                let _ = tx.send(req).await;
                let head = format!(
                    "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nX-Ledger-Request-Id: req-7\r\nX-Ratelimit-Remaining: 42\r\nConnection: close\r\n\r\n",
                    r.status,
                    r.content_type,
                    r.declared_length.unwrap_or(r.body.len())
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(r.body.as_bytes()).await;
            }
        }
    });
    let api = Ledger::new(reqwest::Client::new())
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    (api, rx, task)
}
fn diag() -> Diagnostics {
    Diagnostics {
        request_id: Some("req-7".into()),
        remaining: Some("42".into()),
    }
}

#[tokio::test]
async fn summary_success_has_wire_and_metadata() {
    let (api, mut rx, task) = server(vec![reply(
        "200 OK",
        "application/json",
        r#"{"balance":120}"#,
    )])
    .await;
    let (value, metadata) = summary(&api, "a b").await.unwrap();
    assert_eq!(value, Summary { balance: 120 });
    assert_eq!(metadata, diag());
    let req = rx.recv().await.unwrap();
    assert!(req.head.starts_with("GET /accounts/a%20b/summary HTTP/1.1"));
    assert!(
        req.head
            .to_ascii_lowercase()
            .contains("accept: application/json")
    );
    assert!(req.body.is_empty());
    task.await.unwrap();
}
#[tokio::test]
async fn summary_malformed_and_wrong_media_keep_metadata() {
    for (ct, body, kind) in [
        ("application/json", "{oops", ErrorKind::Decode),
        ("text/plain", "not json", ErrorKind::Metadata),
    ] {
        let (api, _, task) = server(vec![reply("200 OK", ct, body)]).await;
        let err = summary(&api, "1").await.unwrap_err();
        assert_eq!(err.diagnostics, Some(diag()));
        match err.failure {
            SummaryFailure::Concord(e) => assert_eq!(e.kind(), kind),
            other => panic!("{other:?}"),
        };
        task.await.unwrap();
    }
}
#[tokio::test]
async fn summary_service_error_is_typed_with_metadata() {
    let (api, _, task) = server(vec![reply(
        "409 Conflict",
        "application/json",
        r#"{"code":"closed","message":"account closed"}"#,
    )])
    .await;
    let err = summary(&api, "1").await.unwrap_err();
    assert_eq!(err.diagnostics, Some(diag()));
    match err.failure {
        SummaryFailure::Service(p) => assert_eq!(
            p,
            Problem {
                code: "closed".into(),
                message: "account closed".into()
            }
        ),
        other => panic!("{other:?}"),
    };
    task.await.unwrap();
}
#[tokio::test]
async fn runtime_csv_is_lazy_and_matches_both_dialects() {
    for (has_header, delimiter, expected, ct) in [
        (
            true,
            b';',
            "id;amount\r\n1;250\r\n",
            "text/csv; header=present",
        ),
        (false, b'|', "1|250\r\n", "text/csv; header=absent"),
    ] {
        let (api, mut rx, task) = server(vec![reply("204 No Content", "text/plain", "")]).await;
        let polls = Arc::new(AtomicUsize::new(0));
        let seen = polls.clone();
        let rows = std::iter::once_with(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            Ok::<_, Infallible>(Row { id: 1, amount: 250 })
        });
        let upload = csv_upload(
            Dialect {
                header: has_header,
                delimiter,
            },
            stream::from_iter(rows),
        )
        .unwrap();
        let prepared = api.upload_rows("job", upload).prepare().unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        prepared.send().await.unwrap();
        assert_eq!(polls.load(Ordering::SeqCst), 1);
        let req = rx.recv().await.unwrap();
        assert!(req.head.starts_with("POST /jobs/job/rows HTTP/1.1"));
        assert!(
            req.head
                .to_ascii_lowercase()
                .contains(&format!("content-type: {ct}"))
        );
        assert_eq!(req.body, expected.as_bytes());
        task.await.unwrap();
    }
}
#[tokio::test]
async fn archive_streams_through_clean_end() {
    let (api, mut rx, task) = server(vec![
        reply(
            "200 OK",
            "application/jsonl",
            "{\"id\":1,\"cents\":120}\n{\"id\":2,\"cents\":-8}\n",
        ),
        reply("204 No Content", "text/plain", ""),
    ])
    .await;
    let status = archive(&api).await.unwrap();
    assert_eq!(status.completion, Completion::EndOfInput);
    assert!(status.download_error.is_none());
    let get = rx.recv().await.unwrap();
    assert!(get.head.starts_with("GET /entries HTTP/1.1"));
    assert!(
        get.head
            .to_ascii_lowercase()
            .contains("accept: application/jsonl")
    );
    let post = rx.recv().await.unwrap();
    assert!(post.head.starts_with("POST /archive HTTP/1.1"));
    assert!(
        post.head
            .to_ascii_lowercase()
            .contains("content-type: text/csv; header=present")
    );
    assert_eq!(post.body, b"id,amount\r\n1,1.20\r\n2,-0.08\r\n");
    task.await.unwrap();
}
#[tokio::test]
async fn archive_bad_line_names_download_and_is_not_eof() {
    let (api, _, task) = server(vec![
        reply(
            "200 OK",
            "application/jsonl",
            "{\"id\":1,\"cents\":4}\nnot-json\n",
        ),
        reply("204 No Content", "text/plain", ""),
    ])
    .await;
    let err = archive(&api).await.unwrap_err();
    assert!(err.message.contains("Ledger.entries"), "{err:?}");
    assert_ne!(err.status.completion, Completion::EndOfInput);
    assert!(err.status.download_error.is_some());
    task.await.unwrap();
}
#[tokio::test]
async fn archive_truncated_download_is_not_eof() {
    let mut truncated = reply("200 OK", "application/jsonl", "{\"id\":1,\"cents\":4}\n");
    truncated.declared_length = Some(truncated.body.len() + 20);
    let (api, _, task) = server(vec![truncated, reply("204 No Content", "text/plain", "")]).await;
    let err = archive(&api).await.unwrap_err();
    assert_ne!(err.status.completion, Completion::EndOfInput);
    assert!(err.message.contains("Ledger.entries"));
    task.await.unwrap();
}
#[tokio::test]
async fn invoices_exhaustion_and_cap_resume() {
    let first = || {
        reply(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":1},{"id":2}],"next_cursor":"next"}"#,
        )
    };
    let last = || {
        reply(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":3}],"next_cursor":null}"#,
        )
    };
    let (api, mut rx, task) = server(vec![first(), last()]).await;
    let got = all_invoices(api, None)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(
        got,
        vec![Invoice { id: 1 }, Invoice { id: 2 }, Invoice { id: 3 }]
    );
    assert!(
        rx.recv()
            .await
            .unwrap()
            .head
            .starts_with("GET /invoices HTTP/1.1")
    );
    assert!(
        rx.recv()
            .await
            .unwrap()
            .head
            .starts_with("GET /invoices?cursor=next HTTP/1.1")
    );
    task.await.unwrap();
    let (api, mut rx, task) = server(vec![first(), last()]).await;
    let incomplete = all_invoices(api.clone(), NonZeroUsize::new(1))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(
        incomplete.collected,
        vec![Invoice { id: 1 }, Invoice { id: 2 }]
    );
    let resume = match incomplete.error {
        PagesError::Bounded { resume, .. } => resume,
        other => panic!("{other:?}"),
    };
    assert_eq!(resume.key().map(String::as_str), Some("next"));
    assert_eq!(
        all_invoices(api, None)
            .starting_at(resume)
            .items()
            .collect_complete()
            .await
            .unwrap(),
        vec![Invoice { id: 3 }]
    );
    rx.recv().await.unwrap();
    assert!(
        rx.recv()
            .await
            .unwrap()
            .head
            .starts_with("GET /invoices?cursor=next ")
    );
    task.await.unwrap();
}
#[tokio::test]
async fn invoices_failure_retains_prefix_and_failed_cursor() {
    let (api, _, task) = server(vec![
        reply(
            "200 OK",
            "application/json",
            r#"{"data":[{"id":7}],"next_cursor":"bad"}"#,
        ),
        reply(
            "503 Service Unavailable",
            "application/json",
            r#"{"code":"down","message":"retry"}"#,
        ),
    ])
    .await;
    let incomplete = all_invoices(api, None)
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected, vec![Invoice { id: 7 }]);
    match incomplete.error {
        PagesError::Provider {
            error,
            failed: Resume::At(key),
            ..
        } => {
            assert_eq!(key, "bad");
            assert_eq!(error.kind(), ErrorKind::Status);
            assert_eq!(error.status().unwrap().as_u16(), 503);
        }
        other => panic!("{other:?}"),
    }
    task.await.unwrap();
}
