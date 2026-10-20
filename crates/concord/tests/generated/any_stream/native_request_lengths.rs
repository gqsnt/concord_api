//! Native upload completion regressions: the generated API, managed body and
//! actual HTTP framing must agree before an exact-length body completes.

use std::collections::VecDeque;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;

use concord::body::{
    BodyError, BodyFactory, MAX_EMPTY_REQUEST_CHUNKS, PreparedBody, RequestStream,
};
use concord::error::{DispatchKnowledge, ErrorCategory, FailureStage};
use concord::http::SafeProxy;
use concord::retry::{RetryBackoff, RetryDecision, RetryPolicy};
use concord::{Bytes, CancellationToken};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

use super::AnyStreamApi;

const WAIT: Duration = Duration::from_secs(3);

#[derive(Clone, Default)]
struct SourceProbe {
    polls: Arc<AtomicUsize>,
    eof: Arc<AtomicUsize>,
    dropped: Arc<AtomicBool>,
    pending: Arc<Notify>,
}

struct Source {
    frames: VecDeque<Result<Bytes, io::Error>>,
    pending: bool,
    probe: SourceProbe,
}

impl Stream for Source {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.probe.polls.fetch_add(1, Ordering::SeqCst);
        if let Some(frame) = self.frames.pop_front() {
            Poll::Ready(Some(frame))
        } else if self.pending {
            self.probe.pending.notify_one();
            Poll::Pending
        } else {
            self.probe.eof.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(None)
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        self.probe.dropped.store(true, Ordering::SeqCst);
    }
}

fn source(
    frames: Vec<Result<Bytes, io::Error>>,
    exact: u64,
    pending: bool,
) -> (RequestStream, SourceProbe) {
    let probe = SourceProbe::default();
    let stream = RequestStream::from_stream(Source {
        frames: frames.into(),
        pending,
        probe: probe.clone(),
    })
    .exact_length(exact);
    (stream, probe)
}

fn chunks(values: &[&'static [u8]]) -> Vec<Result<Bytes, io::Error>> {
    values
        .iter()
        .map(|value| Ok(Bytes::from_static(value)))
        .collect()
}

fn retry_policy() -> RetryPolicy {
    RetryPolicy::builder()
        .without_budget()
        .max_retries_per_operation(1)
        .backoff(RetryBackoff::fixed(Duration::from_millis(1)))
        .classify_fn(|_| RetryDecision::Retry)
        .build()
        .expect("retry policy")
}

fn client(proxy: SafeProxy, http2: bool) -> AnyStreamApi {
    AnyStreamApi::builder()
        .configure_http(move |http| {
            let http = http.proxy(proxy);
            #[cfg(feature = "http2")]
            if http2 {
                return http.http2_prior_knowledge();
            }
            let _ = http2;
            http.http1_only()
        })
        .retry(retry_policy())
        .build()
        .expect("native exact upload client")
}

async fn released(probe: &SourceProbe) {
    tokio::time::timeout(WAIT, async {
        while !probe.dropped.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("native transport must release its request source");
}

struct Case {
    name: &'static str,
    exact: u64,
    frames: Vec<Result<Bytes, io::Error>>,
    body: Option<&'static [u8]>,
}

fn cases() -> Vec<Case> {
    let mut empty_tail = chunks(&[b"data"]);
    empty_tail.extend((0..MAX_EMPTY_REQUEST_CHUNKS).map(|_| Ok(Bytes::new())));
    let mut excessive_empty_tail = chunks(&[b"data"]);
    excessive_empty_tail.extend((0..=MAX_EMPTY_REQUEST_CHUNKS).map(|_| Ok(Bytes::new())));
    vec![
        Case {
            name: "exact EOF",
            exact: 4,
            frames: chunks(&[b"ab", b"cd"]),
            body: Some(b"abcd"),
        },
        Case {
            name: "zero EOF",
            exact: 0,
            frames: vec![],
            body: Some(b""),
        },
        Case {
            name: "zero with data",
            exact: 0,
            frames: chunks(&[b"data"]),
            body: None,
        },
        Case {
            name: "extra after exact prefix",
            exact: 4,
            frames: chunks(&[b"data", b"extra"]),
            body: None,
        },
        Case {
            name: "trailing producer error",
            exact: 4,
            frames: vec![
                Ok(Bytes::from_static(b"data")),
                Err(io::Error::other("native-body-private-error")),
            ],
            body: None,
        },
        Case {
            name: "first chunk overflow",
            exact: 3,
            frames: chunks(&[b"data"]),
            body: None,
        },
        Case {
            name: "underflow",
            exact: 5,
            frames: chunks(&[b"data"]),
            body: None,
        },
        Case {
            name: "bounded trailing empties",
            exact: 4,
            frames: empty_tail,
            body: Some(b"data"),
        },
        Case {
            name: "excess trailing empties",
            exact: 4,
            frames: excessive_empty_tail,
            body: None,
        },
    ]
}

fn body_failure(error: &concord::ApiError) {
    assert_eq!(error.category(), ErrorCategory::RequestBody);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::PossiblyDispatched
    );
    assert_eq!(error.failure_stage(), FailureStage::DuringDispatchOrUnknown);
    let counts = error.operation_counts().expect("operation journal");
    assert_eq!(counts.application_handoffs(), 1);
    assert_eq!(counts.policy_retries(), 0);
    assert!(!format!("{error:?} {error}").contains("native-body-private-error"));
}

#[tokio::test]
async fn http1_exact_streams_validate_source_completion_at_the_native_boundary() {
    for case in cases() {
        let server = ScriptedServer::start([ResponseScript::ok(b"ok".to_vec())]).expect("server");
        let client = client(crate::support::loopback_proxy(&server), false);
        let (body, probe) = source(case.frames, case.exact, false);
        assert_eq!(probe.polls.load(Ordering::SeqCst), 0);
        let result = client.exact_put(body).response().await;
        match case.body {
            Some(expected) => {
                let response = result.unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
                assert_eq!(response.version(), concord::http::Version::HTTP_11);
                assert_eq!(response.into_value(), b"ok"[..]);
                let requests = server.requests();
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].body(), expected, "{}", case.name);
                if case.exact == 0 {
                    assert_eq!(requests[0].header("content-length"), None);
                    assert_eq!(requests[0].header("transfer-encoding"), Some("chunked"));
                } else {
                    assert_eq!(
                        requests[0].header("content-length"),
                        Some(case.exact.to_string().as_str())
                    );
                }
                assert_eq!(probe.eof.load(Ordering::SeqCst), 1);
            }
            None => {
                body_failure(&result.expect_err(case.name));
                assert_eq!(server.request_count(), 0, "no complete dishonest upload");
            }
        }
        released(&probe).await;
    }
}

struct DishonestFactory(Arc<AtomicUsize>);

impl BodyFactory for DishonestFactory {
    fn create(&self) -> Result<RequestStream, BodyError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(source(chunks(&[b"data", b"extra"]), 4, false).0)
    }
}

#[tokio::test]
async fn exact_factory_failure_is_not_replayed_by_a_retryable_endpoint() {
    let server = ScriptedServer::start([ResponseScript::ok(b"ok".to_vec())]).expect("server");
    let client = client(crate::support::loopback_proxy(&server), false);
    let calls = Arc::new(AtomicUsize::new(0));
    let error = client
        .prepared_upload(PreparedBody::factory(DishonestFactory(Arc::clone(&calls))))
        .await
        .expect_err("body failure must be terminal even with a factory");
    body_failure(&error);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 0);
}

#[tokio::test]
async fn http1_pending_exact_completion_obeys_timeout_and_cancellation() {
    for exact in [0, 4] {
        for cancel in [false, true] {
            let server =
                ScriptedServer::start([ResponseScript::ok(b"ok".to_vec())]).expect("server");
            let client = client(crate::support::loopback_proxy(&server), false);
            let (body, probe) = source(
                if exact == 0 {
                    vec![]
                } else {
                    chunks(&[b"data"])
                },
                exact,
                true,
            );
            let token = CancellationToken::new();
            let request = client
                .exact_put(body)
                .timeout(if cancel {
                    WAIT
                } else {
                    Duration::from_millis(100)
                })
                .expect("finite timeout")
                .cancel_with(token.clone())
                .expect("cancellation");
            let task = tokio::spawn(async move { request.await });
            tokio::time::timeout(WAIT, probe.pending.notified())
                .await
                .expect("source reached pending completion");
            if cancel {
                token.cancel();
            }
            let error = task
                .await
                .expect("request task")
                .expect_err("pending EOF cannot complete an upload");
            assert_eq!(
                error.category(),
                if cancel {
                    ErrorCategory::Cancelled
                } else {
                    ErrorCategory::Timeout
                }
            );
            assert_eq!(
                error.dispatch_knowledge(),
                DispatchKnowledge::PossiblyDispatched
            );
            assert_eq!(probe.eof.load(Ordering::SeqCst), 0);
            assert_eq!(server.request_count(), 0);
            released(&probe).await;
        }
    }
}

#[tokio::test]
async fn http1_early_rejection_does_not_drain_a_pending_exact_source() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("listener");
    let proxy = SafeProxy::parse(format!(
        "http://{}/",
        listener.local_addr().expect("address")
    ))
    .expect("proxy");
    let (body, probe) = source(chunks(&[b"data"]), 4, true);
    let pending = Arc::clone(&probe.pending);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.expect("connection");
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            assert!(head.len() < 65_536, "bounded test request head");
            head.push(socket.read_u8().await.expect("request header"));
        }
        pending.notified().await;
        socket
            .write_all(
                b"HTTP/1.1 413 Payload Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await
            .expect("early rejection");
    });
    let error = tokio::time::timeout(WAIT, client(proxy, false).exact_put(body))
        .await
        .expect("no draining wait")
        .expect_err("server rejection");
    assert_eq!(error.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        error.http_status(),
        Some(concord::http::StatusCode::PAYLOAD_TOO_LARGE)
    );
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(probe.eof.load(Ordering::SeqCst), 0);
    server.await.expect("early server");
    released(&probe).await;
}

#[cfg(feature = "http2")]
mod http2 {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct Observation {
        requests: usize,
        complete: bool,
        length: Option<String>,
        body: Vec<u8>,
    }

    struct Server {
        address: std::net::SocketAddr,
        observed: Arc<Mutex<Observation>>,
        worker: tokio::task::JoinHandle<()>,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.worker.abort();
        }
    }

    impl Server {
        fn proxy(&self) -> SafeProxy {
            SafeProxy::parse(format!("http://{}/", self.address)).expect("proxy")
        }

        async fn start(early: Option<Arc<Notify>>) -> Self {
            let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
                .await
                .expect("H2 listener");
            let address = listener.local_addr().expect("address");
            let observed = Arc::new(Mutex::new(Observation::default()));
            let record = Arc::clone(&observed);
            let worker = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.expect("H2 connection");
                let mut connection = h2::server::handshake(socket).await.expect("H2 handshake");
                let mut handlers = tokio::task::JoinSet::new();
                while let Some(request) = connection.accept().await {
                    let Ok((mut request, mut respond)) = request else {
                        break;
                    };
                    {
                        let mut record = record.lock().expect("observation");
                        record.requests += 1;
                        record.length = request
                            .headers()
                            .get("content-length")
                            .map(|value| value.to_str().expect("length").to_owned());
                    }
                    let record = Arc::clone(&record);
                    let early = early.clone();
                    handlers.spawn(async move {
                        if let Some(pending) = early {
                            pending.notified().await;
                            let response = http::Response::builder()
                                .status(413)
                                .body(())
                                .expect("response");
                            respond
                                .send_response(response, true)
                                .expect("early H2 response");
                            return;
                        }
                        while let Some(chunk) = request.body_mut().data().await {
                            let Ok(chunk) = chunk else {
                                return;
                            };
                            request
                                .body_mut()
                                .flow_control()
                                .release_capacity(chunk.len())
                                .expect("H2 capacity");
                            let mut record = record.lock().expect("observation");
                            assert!(
                                record.body.len() + chunk.len() <= 65_536,
                                "bounded test upload"
                            );
                            record.body.extend_from_slice(&chunk);
                        }
                        record.lock().expect("observation").complete = true;
                        let response = http::Response::builder()
                            .status(200)
                            .body(())
                            .expect("response");
                        let mut send = respond.send_response(response, false).expect("H2 response");
                        send.send_data(Bytes::from_static(b"ok"), true)
                            .expect("H2 response data");
                    });
                }
            });
            Self {
                address,
                observed,
                worker,
            }
        }
    }

    #[tokio::test]
    async fn http2_exact_streams_validate_source_completion_at_the_native_boundary() {
        for case in cases() {
            let server = Server::start(None).await;
            let client = client(server.proxy(), true);
            let (body, probe) = source(case.frames, case.exact, false);
            assert_eq!(probe.polls.load(Ordering::SeqCst), 0);
            let result = client.exact_put(body).response().await;
            match case.body {
                Some(expected) => {
                    let response =
                        result.unwrap_or_else(|error| panic!("{}: {error:?}", case.name));
                    assert_eq!(response.version(), concord::http::Version::HTTP_2);
                    assert_eq!(response.into_value(), b"ok"[..]);
                    let observation = server.observed.lock().expect("observation");
                    assert_eq!(observation.requests, 1);
                    assert!(observation.complete);
                    assert_eq!(observation.body, expected, "{}", case.name);
                    assert_eq!(
                        observation.length,
                        (case.exact != 0).then(|| case.exact.to_string())
                    );
                    assert_eq!(probe.eof.load(Ordering::SeqCst), 1);
                }
                None => {
                    body_failure(&result.expect_err(case.name));
                    assert!(!server.observed.lock().expect("observation").complete);
                }
            }
            released(&probe).await;
        }
    }

    #[tokio::test]
    async fn http2_pending_exact_completion_obeys_timeout_and_cancellation() {
        for exact in [0, 4] {
            for cancel in [false, true] {
                let server = Server::start(None).await;
                let client = client(server.proxy(), true);
                let (body, probe) = source(
                    if exact == 0 {
                        vec![]
                    } else {
                        chunks(&[b"data"])
                    },
                    exact,
                    true,
                );
                let token = CancellationToken::new();
                let request = client
                    .exact_put(body)
                    .timeout(if cancel {
                        WAIT
                    } else {
                        Duration::from_millis(100)
                    })
                    .expect("finite timeout")
                    .cancel_with(token.clone())
                    .expect("cancellation");
                let task = tokio::spawn(async move { request.await });
                tokio::time::timeout(WAIT, probe.pending.notified())
                    .await
                    .expect("pending H2 completion");
                if cancel {
                    token.cancel();
                }
                let error = task
                    .await
                    .expect("request task")
                    .expect_err("pending H2 EOF");
                assert_eq!(
                    error.category(),
                    if cancel {
                        ErrorCategory::Cancelled
                    } else {
                        ErrorCategory::Timeout
                    }
                );
                assert_eq!(
                    error.dispatch_knowledge(),
                    DispatchKnowledge::PossiblyDispatched
                );
                assert_eq!(probe.eof.load(Ordering::SeqCst), 0);
                assert!(!server.observed.lock().expect("observation").complete);
                released(&probe).await;
            }
        }
    }

    #[tokio::test]
    async fn http2_early_rejection_does_not_drain_a_pending_exact_source() {
        let (body, probe) = source(chunks(&[b"data"]), 4, true);
        let server = Server::start(Some(Arc::clone(&probe.pending))).await;
        let error = tokio::time::timeout(WAIT, client(server.proxy(), true).exact_put(body))
            .await
            .expect("no H2 draining wait")
            .expect_err("server rejection");
        assert_eq!(error.category(), ErrorCategory::HttpStatus);
        assert_eq!(
            error.http_status(),
            Some(concord::http::StatusCode::PAYLOAD_TOO_LARGE)
        );
        assert_eq!(
            error.dispatch_knowledge(),
            DispatchKnowledge::ResponseObserved
        );
        assert_eq!(probe.eof.load(Ordering::SeqCst), 0);
        assert!(!server.observed.lock().expect("observation").complete);
        released(&probe).await;
    }
}
