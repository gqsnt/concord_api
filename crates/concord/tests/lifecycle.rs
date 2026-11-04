//! Bounded observations of preparation, the response stages and cancellation ownership.
#![allow(missing_docs)]

use concord::{CodecError, Decode, ErrorKind, ResponseHead, bytes::Bytes, reqwest};
use std::{sync::Mutex, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, oneshot},
};

const LIMIT: Duration = Duration::from_secs(3);
static EVENTS: Mutex<Option<mpsc::UnboundedSender<&'static str>>> = Mutex::new(None);
fn record(event: &'static str) {
    EVENTS
        .lock()
        .unwrap()
        .as_ref()
        .unwrap()
        .send(event)
        .unwrap();
}
struct State;
impl Drop for State {
    fn drop(&mut self) {
        record("drop");
    }
}
struct Format;
impl Decode<String> for Format {
    type Context = State;
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        record("check");
        if head.headers().contains_key("x-refuse") {
            Err(CodecError::new("refused"))
        } else {
            Ok(())
        }
    }
    fn context(_: &ResponseHead<'_>) -> Result<State, CodecError> {
        record("construct");
        Ok(State)
    }
    fn decode_body(_state: State, body: Bytes) -> Result<String, CodecError> {
        record("decode");
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("bad utf8"))
    }
}
concord::api! {
    client Api {
        base "http://localhost";
        GET item() -> String { response Format; }
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Head,
    Body,
    Success,
    Short,
    Refuse,
    Status,
}
async fn fixture(
    mode: Mode,
) -> (
    Api,
    mpsc::UnboundedReceiver<&'static str>,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<usize>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (events, rx) = mpsc::unbounded_channel();
    let (stop, mut stopped) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(LIMIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            assert_eq!(
                tokio::time::timeout(LIMIT, socket.read(&mut byte))
                    .await
                    .unwrap()
                    .unwrap(),
                1
            );
            head.push(byte[0]);
        }
        events.send("request").unwrap();
        if !matches!(mode, Mode::Head) {
            let head = match mode {
                Mode::Refuse => "HTTP/1.1 200 OK\r\nx-refuse: yes\r\ncontent-length: 2\r\n\r\n",
                Mode::Status => {
                    "HTTP/1.1 503 Unavailable\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                }
                _ => "HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n",
            };
            socket.write_all(head.as_bytes()).await.unwrap();
            if matches!(mode, Mode::Success) {
                socket.write_all(b"ok").await.unwrap();
            }
            if matches!(mode, Mode::Short) {
                socket.write_all(b"x").await.unwrap();
            }
            if matches!(mode, Mode::Short | Mode::Status) {
                drop(socket);
            }
        }
        let mut count = 1;
        loop {
            tokio::select! {
                _ = &mut stopped => {
                    if tokio::time::timeout(Duration::from_millis(100), listener.accept()).await.is_ok() { count += 1; }
                    break;
                },
                result = listener.accept() => { let _ = result.unwrap(); count += 1; }
                _ = tokio::time::sleep(LIMIT * 2) => panic!("server shutdown timed out"),
            }
        }
        count
    });
    let http = reqwest::Client::builder()
        .no_proxy()
        .http1_only()
        .timeout(LIMIT * 2)
        .build()
        .unwrap();
    let api = Api::new(http)
        .with_base_url(format!("http://{addr}"))
        .unwrap();
    (api, rx, stop, server)
}
async fn event(rx: &mut mpsc::UnboundedReceiver<&'static str>, expected: &'static str) {
    assert_eq!(
        tokio::time::timeout(LIMIT, rx.recv())
            .await
            .expect("event timed out"),
        Some(expected)
    );
}
async fn finish(stop: oneshot::Sender<()>, server: tokio::task::JoinHandle<usize>) {
    stop.send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(LIMIT, server).await.unwrap().unwrap(),
        1,
        "one send issued one request"
    );
}

#[tokio::test]
async fn stages_cancellation_and_controls() {
    let (tx, mut format_events) = mpsc::unbounded_channel();
    *EVENTS.lock().unwrap() = Some(tx);

    let (api, mut requests, stop, server) = fixture(Mode::Head).await;
    let prepared = api.item().prepare().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), requests.recv())
            .await
            .is_err(),
        "prepare performed I/O"
    );
    let task = tokio::spawn(prepared.send());
    event(&mut requests, "request").await;
    assert!(!task.is_finished());
    assert!(format_events.try_recv().is_err());
    task.abort();
    assert!(
        tokio::time::timeout(LIMIT, task)
            .await
            .unwrap()
            .unwrap_err()
            .is_cancelled()
    );
    assert!(format_events.try_recv().is_err());
    finish(stop, server).await;

    let (api, mut requests, stop, server) = fixture(Mode::Body).await;
    let task = tokio::spawn(api.item().prepare().unwrap().send());
    event(&mut requests, "request").await;
    event(&mut format_events, "check").await;
    event(&mut format_events, "construct").await;
    assert!(!task.is_finished());
    assert!(
        format_events.try_recv().is_err(),
        "context must remain alive while body is pending"
    );
    task.abort();
    assert!(
        tokio::time::timeout(LIMIT, task)
            .await
            .unwrap()
            .unwrap_err()
            .is_cancelled()
    );
    event(&mut format_events, "drop").await;
    assert!(format_events.try_recv().is_err());
    finish(stop, server).await;

    for mode in [Mode::Success, Mode::Short, Mode::Refuse, Mode::Status] {
        let (api, mut requests, stop, server) = fixture(mode).await;
        let result = tokio::time::timeout(LIMIT, api.item().send())
            .await
            .unwrap();
        event(&mut requests, "request").await;
        match mode {
            Mode::Success => {
                assert_eq!(result.unwrap(), "ok");
                event(&mut format_events, "check").await;
                event(&mut format_events, "construct").await;
                event(&mut format_events, "decode").await;
                event(&mut format_events, "drop").await;
            }
            Mode::Short => {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Transport);
                event(&mut format_events, "check").await;
                event(&mut format_events, "construct").await;
                event(&mut format_events, "drop").await;
            }
            Mode::Refuse => {
                assert_eq!(result.unwrap_err().kind(), ErrorKind::Metadata);
                event(&mut format_events, "check").await;
            }
            Mode::Status => assert_eq!(result.unwrap_err().kind(), ErrorKind::Status),
            _ => unreachable!(),
        }
        assert!(format_events.try_recv().is_err());
        finish(stop, server).await;
    }
    *EVENTS.lock().unwrap() = None;
}
