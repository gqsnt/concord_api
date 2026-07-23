use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::pin::Pin;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use concord_test_support::proxy::ProxyTrap;
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use futures_core::Stream;
use h2::Reason;
use tokio::net::TcpListener as TokioTcpListener;

const PROBE_WAIT: Duration = Duration::from_secs(3);

fn install_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn hardened_builder() -> reqwest::ClientBuilder {
    install_crypto_provider();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .retry(reqwest::retry::never())
}

fn hardened_client() -> reqwest::Client {
    hardened_builder()
        .http1_only()
        .build()
        .expect("hardened loopback client should build")
}

#[tokio::test]
async fn redirects_are_disabled() {
    let server = ScriptedServer::start([
        ResponseScript::status(302, Vec::new()).with_header("Location", "/target"),
        ResponseScript::ok(b"unexpected target".to_vec()),
    ])
    .expect("redirect origin should start");

    let response = hardened_client()
        .get(server.url("/redirect"))
        .send()
        .await
        .expect("redirect response should be returned");

    assert_eq!(response.status(), reqwest::StatusCode::FOUND);
    server
        .wait_for_requests(1, PROBE_WAIT)
        .expect("redirect request should be recorded");
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].target(), "/redirect");
}

#[tokio::test]
async fn system_proxy_environment_is_ignored() {
    if std::env::var_os("CONCORD_SYSTEM_PROXY_CHILD").is_some() {
        let origin: SocketAddr = std::env::var("CONCORD_ORIGIN_ADDR")
            .expect("child origin address should exist")
            .parse()
            .expect("child origin address should parse");
        let client = hardened_builder()
            .resolve("probe.invalid", origin)
            .build()
            .expect("child hardened client should build");
        let response = client
            .get(format!("http://probe.invalid:{}/direct", origin.port()))
            .send()
            .await
            .expect("hardened client should bypass environment proxy");
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        return;
    }

    let origin = ScriptedServer::start([ResponseScript::ok(b"direct".to_vec())])
        .expect("direct origin should start");
    let proxy = ProxyTrap::start([ResponseScript::ok(b"proxy".to_vec())])
        .expect("environment proxy trap should start");
    let output = child_command("system_proxy_environment_is_ignored")
        .env("CONCORD_SYSTEM_PROXY_CHILD", "1")
        .env("CONCORD_ORIGIN_ADDR", origin.address().to_string())
        .env("HTTP_PROXY", proxy.url())
        .env("HTTPS_PROXY", proxy.url())
        .env("ALL_PROXY", proxy.url())
        .env("NO_PROXY", "")
        .env("http_proxy", proxy.url())
        .env("https_proxy", proxy.url())
        .env("all_proxy", proxy.url())
        .env("no_proxy", "")
        .output()
        .expect("system-proxy child should run");
    assert_child_success(&output);
    origin
        .wait_for_requests(1, PROBE_WAIT)
        .expect("direct origin should be reached");
    assert_eq!(proxy.request_count(), 0);
}

#[tokio::test]
async fn explicit_proxy_is_used_only_when_configured() {
    if std::env::var_os("CONCORD_EXPLICIT_PROXY_CHILD").is_some() {
        let origin = child_socket("CONCORD_ORIGIN_ADDR");
        let explicit =
            std::env::var("CONCORD_EXPLICIT_PROXY").expect("explicit proxy URL should exist");
        let target = format!("http://probe.invalid:{}/resource", origin.port());
        let explicit_client = hardened_builder()
            .proxy(reqwest::Proxy::all(explicit).expect("explicit proxy should parse"))
            .resolve("probe.invalid", origin)
            .build()
            .expect("explicit proxy client should build");
        let proxied = explicit_client
            .get(&target)
            .send()
            .await
            .expect("explicit proxy should answer");
        assert_eq!(proxied.status(), reqwest::StatusCode::OK);

        let direct = hardened_builder()
            .resolve("probe.invalid", origin)
            .build()
            .expect("direct client should build")
            .get(target)
            .send()
            .await
            .expect("ordinary hardened client should remain direct");
        assert_eq!(direct.status(), reqwest::StatusCode::OK);
        return;
    }

    let origin = ScriptedServer::start([ResponseScript::ok(b"direct".to_vec())])
        .expect("origin should start");
    let explicit = ProxyTrap::start([ResponseScript::ok(b"explicit".to_vec())])
        .expect("explicit proxy should start");
    let unrelated = ProxyTrap::start([ResponseScript::ok(b"unrelated".to_vec())])
        .expect("unrelated proxy should start");
    let output = child_command("explicit_proxy_is_used_only_when_configured")
        .env("CONCORD_EXPLICIT_PROXY_CHILD", "1")
        .env("CONCORD_ORIGIN_ADDR", origin.address().to_string())
        .env("CONCORD_EXPLICIT_PROXY", explicit.url())
        .env("HTTP_PROXY", unrelated.url())
        .env("HTTPS_PROXY", unrelated.url())
        .env("ALL_PROXY", unrelated.url())
        .env("NO_PROXY", "")
        .output()
        .expect("explicit-proxy child should run");
    assert_child_success(&output);
    explicit
        .wait_for_requests(1, PROBE_WAIT)
        .expect("explicit proxy should be contacted");
    origin
        .wait_for_requests(1, PROBE_WAIT)
        .expect("ordinary client should contact origin");
    assert_eq!(explicit.request_count(), 1);
    assert!(
        explicit.requests()[0]
            .target()
            .starts_with("http://probe.invalid:")
    );
    assert_eq!(origin.request_count(), 1);
    assert_eq!(unrelated.request_count(), 0);
}

#[tokio::test]
async fn cookies_are_not_replayed() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"set".to_vec())
            .with_header("Set-Cookie", "probe_cookie=not-a-credential; Path=/"),
        ResponseScript::ok(b"check".to_vec()),
    ])
    .expect("cookie origin should start");
    let client = hardened_client();
    client
        .get(server.url("/set"))
        .send()
        .await
        .expect("cookie-setting response should arrive")
        .bytes()
        .await
        .expect("cookie-setting body should complete");
    client
        .get(server.url("/check"))
        .send()
        .await
        .expect("cookie-check response should arrive")
        .bytes()
        .await
        .expect("cookie-check body should complete");
    server
        .wait_for_requests(2, PROBE_WAIT)
        .expect("both cookie requests should be recorded");
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].header("cookie"), None);
}

struct H2RefusedServer {
    address: SocketAddr,
    physical_attempts: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
    worker: tokio::task::JoinHandle<()>,
}

impl H2RefusedServer {
    async fn start(refusals: usize, expected_attempts: usize) -> Self {
        let listener = TokioTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .expect("HTTP/2 retry listener should bind");
        let address = listener.local_addr().expect("HTTP/2 address should exist");
        let physical_attempts = Arc::new(AtomicUsize::new(0));
        let connections = Arc::new(AtomicUsize::new(0));
        let attempt_counter = Arc::clone(&physical_attempts);
        let connection_counter = Arc::clone(&connections);
        let worker = tokio::spawn(async move {
            let (stream, _) = tokio::time::timeout(PROBE_WAIT, listener.accept())
                .await
                .expect("HTTP/2 client did not connect in time")
                .expect("HTTP/2 accept should succeed");
            connection_counter.fetch_add(1, Ordering::SeqCst);
            let mut connection = h2::server::handshake(stream)
                .await
                .expect("HTTP/2 handshake should succeed");
            while let Some(request) = tokio::time::timeout(PROBE_WAIT, connection.accept())
                .await
                .expect("HTTP/2 request did not arrive in time")
            {
                let (_request, mut respond) = request.expect("HTTP/2 request should parse");
                let attempt = attempt_counter.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt <= refusals {
                    respond.send_reset(Reason::REFUSED_STREAM);
                } else {
                    let response = http::Response::builder()
                        .status(200)
                        .body(())
                        .expect("HTTP/2 response should build");
                    respond
                        .send_response(response, true)
                        .expect("HTTP/2 response should send");
                }
                if attempt >= expected_attempts {
                    // Polling the connection once more drives the queued RESET or
                    // response frame onto the socket before this probe drops it.
                    let _ =
                        tokio::time::timeout(Duration::from_millis(500), connection.accept()).await;
                    break;
                }
            }
        });
        Self {
            address,
            physical_attempts,
            connections,
            worker,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/retry", self.address)
    }

    async fn finish(self) {
        tokio::time::timeout(PROBE_WAIT, self.worker)
            .await
            .expect("HTTP/2 retry worker should finish in time")
            .expect("HTTP/2 retry worker should not panic");
    }
}

fn h2_builder() -> reqwest::ClientBuilder {
    hardened_builder().http2_prior_knowledge()
}

#[tokio::test]
async fn retry_is_off_by_default() {
    let server = H2RefusedServer::start(1, 1).await;
    let attempts = Arc::clone(&server.physical_attempts);
    let connections = Arc::clone(&server.connections);
    let result = h2_builder()
        .build()
        .expect("retry-free HTTP/2 client should build")
        .get(server.url())
        .send()
        .await;
    assert!(result.is_err());
    server.finish().await;
    let physical = attempts.load(Ordering::SeqCst);
    let connection_count = connections.load(Ordering::SeqCst);
    assert_eq!(physical, 1);
    assert_eq!(connection_count, 1);
    println!("retry_off_physical_attempts={physical} connections={connection_count}");
}

#[tokio::test]
async fn reqwest_default_retry_requires_explicit_opt_in() {
    let server = H2RefusedServer::start(1, 2).await;
    let attempts = Arc::clone(&server.physical_attempts);
    let connections = Arc::clone(&server.connections);
    install_crypto_provider();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .http2_prior_knowledge()
        .build()
        .expect("Reqwest-default retry client should build");
    let response = client
        .get(server.url())
        .send()
        .await
        .expect("Reqwest default policy should retry REFUSED_STREAM");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    server.finish().await;
    let physical = attempts.load(Ordering::SeqCst);
    let connection_count = connections.load(Ordering::SeqCst);
    assert!(physical > 1);
    assert_eq!(connection_count, 1);
    println!("reqwest_default_physical_attempts={physical} connections={connection_count}");
}

#[derive(Default)]
struct BoundaryCounters {
    hook: AtomicUsize,
    limiter: AtomicUsize,
    body_factory: AtomicUsize,
    auth_finalization: AtomicUsize,
    logical_execution: AtomicUsize,
    reqwest_call: AtomicUsize,
}

async fn execute_one_concord_attempt<Hook, Limiter, BodyFactory, AuthFinalization>(
    client: &reqwest::Client,
    url: String,
    counters: &BoundaryCounters,
    hook: Hook,
    limiter: Limiter,
    body_factory: BodyFactory,
    auth_finalization: AuthFinalization,
) -> Result<reqwest::Response, reqwest::Error>
where
    Hook: FnOnce(),
    Limiter: FnOnce(),
    BodyFactory: FnOnce() -> Vec<u8>,
    AuthFinalization: FnOnce(),
{
    counters.logical_execution.fetch_add(1, Ordering::SeqCst);
    hook();
    counters.hook.fetch_add(1, Ordering::SeqCst);
    limiter();
    counters.limiter.fetch_add(1, Ordering::SeqCst);
    let body = body_factory();
    counters.body_factory.fetch_add(1, Ordering::SeqCst);
    auth_finalization();
    counters.auth_finalization.fetch_add(1, Ordering::SeqCst);
    counters.reqwest_call.fetch_add(1, Ordering::SeqCst);
    client.post(url).body(body).send().await
}

#[tokio::test]
async fn hidden_retry_does_not_reenter_concord_boundaries() {
    let server = H2RefusedServer::start(1, 2).await;
    let physical = Arc::clone(&server.physical_attempts);
    let counters = BoundaryCounters::default();
    install_crypto_provider();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .http2_prior_knowledge()
        .build()
        .expect("Reqwest-default retry client should build");
    let response = execute_one_concord_attempt(
        &client,
        server.url(),
        &counters,
        || {},
        || {},
        || Vec::from(&b"inert-repeatable-body"[..]),
        || {},
    )
    .await
    .expect("Reqwest should replay its reusable body internally");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    server.finish().await;
    assert_eq!(counters.logical_execution.load(Ordering::SeqCst), 1);
    assert_eq!(counters.hook.load(Ordering::SeqCst), 1);
    assert_eq!(counters.limiter.load(Ordering::SeqCst), 1);
    assert_eq!(counters.body_factory.load(Ordering::SeqCst), 1);
    assert_eq!(counters.auth_finalization.load(Ordering::SeqCst), 1);
    assert_eq!(counters.reqwest_call.load(Ordering::SeqCst), 1);
    let physical = physical.load(Ordering::SeqCst);
    assert!(physical > 1);
    println!(
        "hidden_retry_logical=1 hook=1 limiter=1 body_factory=1 auth_finalization=1 reqwest_call=1 physical={physical}"
    );
}

struct NativePollProbe {
    item: Option<Result<Bytes, io::Error>>,
    polls: Arc<AtomicUsize>,
    dropped: Option<Arc<AtomicBool>>,
    reached: Option<Arc<tokio::sync::Notify>>,
    pending: bool,
}

impl Stream for NativePollProbe {
    type Item = Result<Bytes, io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        if let Some(reached) = &self.reached {
            reached.notify_one();
        }
        if self.pending {
            Poll::Pending
        } else {
            Poll::Ready(self.item.take())
        }
    }
}

impl Drop for NativePollProbe {
    fn drop(&mut self) {
        if let Some(dropped) = &self.dropped {
            dropped.store(true, Ordering::SeqCst);
        }
    }
}

#[tokio::test]
async fn reqwest_stream_polling_starts_only_when_send_is_polled() {
    let server = ScriptedServer::start([ResponseScript::ok(b"accepted".to_vec())])
        .expect("native request stream server should start");
    let polls = Arc::new(AtomicUsize::new(0));
    let body = reqwest::Body::wrap_stream(NativePollProbe {
        item: Some(Ok(Bytes::from_static(b"probe"))),
        polls: Arc::clone(&polls),
        dropped: None,
        reached: None,
        pending: false,
    });
    let future = hardened_client()
        .post(server.url("/native-stream"))
        .header(reqwest::header::CONTENT_LENGTH, 5)
        .body(body)
        .send();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    let response = future.await.expect("native stream response");
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(polls.load(Ordering::SeqCst), 1);
    server
        .wait_for_requests(1, PROBE_WAIT)
        .expect("native stream request should be recorded");
    assert_eq!(server.requests()[0].body(), b"probe");
}

#[cfg(feature = "multipart")]
#[tokio::test]
async fn reqwest_owns_multipart_boundary_length_and_stream_polling() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"buffered".to_vec()),
        ResponseScript::ok(b"streamed".to_vec()),
    ])
    .expect("native multipart server should start");
    let client = hardened_client();
    let buffered = reqwest::multipart::Form::new().part(
        "field",
        reqwest::multipart::Part::text("native-multipart-buffered-probe"),
    );
    client
        .post(server.url("/native-multipart-buffered"))
        .multipart(buffered)
        .send()
        .await
        .expect("buffered native multipart request");

    let polls = Arc::new(AtomicUsize::new(0));
    let body = reqwest::Body::wrap_stream(NativePollProbe {
        item: Some(Ok(Bytes::from_static(b"native-multipart-stream-probe"))),
        polls: Arc::clone(&polls),
        dropped: None,
        reached: None,
        pending: false,
    });
    let streamed =
        reqwest::multipart::Form::new().part("stream", reqwest::multipart::Part::stream(body));
    let future = client
        .post(server.url("/native-multipart-streamed"))
        .multipart(streamed)
        .send();
    assert_eq!(polls.load(Ordering::SeqCst), 0);
    future.await.expect("streamed native multipart request");

    server
        .wait_for_requests(2, PROBE_WAIT)
        .expect("native multipart requests should be recorded");
    let requests = server.requests();
    for request in &requests {
        assert!(
            request
                .header("content-type")
                .is_some_and(|value| value.starts_with("multipart/form-data; boundary="))
        );
    }
    assert!(requests[0].header("content-length").is_some());
    assert!(
        String::from_utf8_lossy(requests[0].body()).contains("native-multipart-buffered-probe")
    );
    assert_eq!(requests[1].header("transfer-encoding"), Some("chunked"));
    assert!(String::from_utf8_lossy(requests[1].body()).contains("native-multipart-stream-probe"));
    assert_eq!(polls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn reqwest_propagates_request_stream_producer_failure() {
    let listener = TokioTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("producer failure listener should bind");
    let address = listener.local_addr().expect("producer listener address");
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("producer peer accept");
        std::future::pending::<()>().await;
    });
    let polls = Arc::new(AtomicUsize::new(0));
    let body = reqwest::Body::wrap_stream(NativePollProbe {
        item: Some(Err(io::Error::other("upstream-producer-probe"))),
        polls: Arc::clone(&polls),
        dropped: None,
        reached: None,
        pending: false,
    });
    let error = hardened_client()
        .post(format!("http://{address}/producer-failure"))
        .header(reqwest::header::CONTENT_LENGTH, 1)
        .body(body)
        .send()
        .await
        .expect_err("producer failure should reach Reqwest");
    peer.abort();
    let _ = peer.await;
    assert!(error.is_body() || error.is_request());
    assert_eq!(polls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dropping_reqwest_send_drops_pending_request_stream() {
    let listener = TokioTcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("cancellation listener should bind");
    let address = listener
        .local_addr()
        .expect("cancellation listener address");
    let peer = tokio::spawn(async move {
        let (_socket, _) = listener.accept().await.expect("cancellation peer accept");
        std::future::pending::<()>().await;
    });
    let polls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let reached = Arc::new(tokio::sync::Notify::new());
    let body = reqwest::Body::wrap_stream(NativePollProbe {
        item: None,
        polls: Arc::clone(&polls),
        dropped: Some(Arc::clone(&dropped)),
        reached: Some(Arc::clone(&reached)),
        pending: true,
    });
    let client = hardened_client();
    let task = tokio::spawn(async move {
        client
            .post(format!("http://{address}/cancel-stream"))
            .header(reqwest::header::CONTENT_LENGTH, 1)
            .body(body)
            .send()
            .await
    });
    tokio::time::timeout(PROBE_WAIT, reached.notified())
        .await
        .expect("Reqwest should poll pending request stream");
    let polls_at_drop = polls.load(Ordering::SeqCst);
    task.abort();
    let join = task.await.expect_err("send task should be cancelled");
    assert!(join.is_cancelled());
    peer.abort();
    let _ = peer.await;
    assert!(dropped.load(Ordering::SeqCst));
    tokio::task::yield_now().await;
    assert_eq!(polls.load(Ordering::SeqCst), polls_at_drop);
}

#[derive(Debug, Eq, PartialEq)]
enum DeadlineClass {
    NativeReqwestTimeout,
    OperationDeadline,
}

#[tokio::test]
async fn native_timeout_is_distinct_from_operation_deadline() {
    let native_server = ScriptedServer::start([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_millis(350),
        status: 200,
        body: b"late".to_vec(),
    }])
    .expect("native-timeout server should start");
    let native_client = hardened_builder()
        .timeout(Duration::from_millis(120))
        .build()
        .expect("native-timeout client should build");
    let started = Instant::now();
    let error = native_client
        .get(native_server.url("/native-timeout"))
        .send()
        .await
        .expect_err("native timeout should fire");
    let native_elapsed = started.elapsed();
    assert!(error.is_timeout());
    assert!((Duration::from_millis(60)..Duration::from_secs(2)).contains(&native_elapsed));
    let native_class = DeadlineClass::NativeReqwestTimeout;

    let deadline_server = ScriptedServer::start([ResponseScript::DelayBeforeHeaders {
        delay: Duration::from_millis(350),
        status: 200,
        body: b"late".to_vec(),
    }])
    .expect("operation-deadline server should start");
    let deadline_client = hardened_builder()
        .timeout(Duration::from_secs(1))
        .build()
        .expect("operation-deadline client should build");
    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_millis(80),
        deadline_client
            .get(deadline_server.url("/operation-deadline"))
            .send(),
    )
    .await;
    let deadline_elapsed = started.elapsed();
    assert!(result.is_err());
    assert!((Duration::from_millis(40)..Duration::from_secs(1)).contains(&deadline_elapsed));
    let deadline_class = DeadlineClass::OperationDeadline;
    assert_ne!(native_class, deadline_class);
    println!(
        "native_timeout_observed_ms={} operation_deadline_observed_ms={}",
        native_elapsed.as_millis(),
        deadline_elapsed.as_millis()
    );
}

#[derive(Debug, Eq, PartialEq)]
enum FailureClass {
    Connect,
    BodyRead { status: u16 },
}

#[tokio::test]
async fn connect_failure_is_distinct_from_read_failure() {
    let reserved =
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("closed-port reservation should bind");
    let address = reserved
        .local_addr()
        .expect("reserved address should exist");
    drop(reserved);
    let connect_error = hardened_builder()
        .connect_timeout(Duration::from_millis(500))
        .build()
        .expect("connect-failure client should build")
        .get(format!("http://{address}/connect"))
        .send()
        .await
        .expect_err("closed loopback port should reject connection");
    assert!(connect_error.is_connect());
    let connect_class = FailureClass::Connect;

    let server = ScriptedServer::start([ResponseScript::Truncated {
        status: 500,
        declared_length: 32,
        prefix: b"partial-marker".to_vec(),
    }])
    .expect("truncated-body server should start");
    let response = hardened_client()
        .get(server.url("/truncated"))
        .send()
        .await
        .expect("status should arrive before truncation");
    let status = response.status().as_u16();
    let read_error = response
        .bytes()
        .await
        .expect_err("truncated body should fail complete read");
    assert!(!read_error.is_connect());
    assert!(!read_error.is_timeout());
    let read_class = FailureClass::BodyRead { status };
    assert_ne!(connect_class, read_class);
    assert_eq!(read_class, FailureClass::BodyRead { status: 500 });
}

#[tokio::test]
async fn small_complete_body_permits_http11_reuse() {
    let server = ScriptedServer::start([
        ResponseScript::ok(b"one".to_vec()),
        ResponseScript::ok(b"two".to_vec()),
    ])
    .expect("reuse server should start");
    let client = hardened_client();
    let first = client
        .get(server.url("/one"))
        .send()
        .await
        .expect("first response should arrive")
        .bytes()
        .await
        .expect("first body should complete");
    let second = client
        .get(server.url("/two"))
        .send()
        .await
        .expect("second response should arrive")
        .bytes()
        .await
        .expect("second body should complete");
    assert_eq!(first.as_ref(), b"one");
    assert_eq!(second.as_ref(), b"two");
    let records = server.requests();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].connection_id(), records[1].connection_id());
}

#[tokio::test]
async fn unread_response_drop_is_observed() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedBody {
            status: 500,
            body: vec![b'x'; 32 * 1024],
            gate: gate.clone(),
        },
        ResponseScript::ok(b"after".to_vec()),
    ])
    .expect("unread-body server should start");
    let client = hardened_client();
    let response = client
        .get(server.url("/unread"))
        .send()
        .await
        .expect("unread response headers should arrive");
    assert_eq!(
        response.status(),
        reqwest::StatusCode::INTERNAL_SERVER_ERROR
    );
    drop(response);
    gate.release();
    client
        .get(server.url("/after"))
        .send()
        .await
        .expect("request after unread drop should complete")
        .bytes()
        .await
        .expect("follow-up body should complete");
    let records = server.requests();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0].connection_id(), records[1].connection_id());
}

#[tokio::test]
async fn known_oversized_abandon_is_observed() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedBody {
            status: 500,
            body: vec![b'k'; 32 * 1024],
            gate: gate.clone(),
        },
        ResponseScript::ok(b"after".to_vec()),
    ])
    .expect("known-oversized server should start");
    let client = hardened_client();
    let response = client
        .get(server.url("/known"))
        .send()
        .await
        .expect("known-size headers should arrive");
    assert_eq!(response.content_length(), Some(32 * 1024));
    drop(response);
    gate.release();
    client
        .get(server.url("/after"))
        .send()
        .await
        .expect("follow-up should complete")
        .bytes()
        .await
        .expect("follow-up body should complete");
    let records = server.requests();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0].connection_id(), records[1].connection_id());
}

#[tokio::test]
async fn unknown_oversized_abandon_is_observed() {
    let gate = BodyGate::closed();
    let server = ScriptedServer::start([
        ResponseScript::GatedChunked {
            status: 500,
            first_chunk: vec![b'u'; 1025],
            remaining_chunk: vec![b'v'; 16 * 1024],
            gate: gate.clone(),
        },
        ResponseScript::ok(b"after".to_vec()),
    ])
    .expect("unknown-oversized server should start");
    let client = hardened_client();
    let mut response = client
        .get(server.url("/unknown"))
        .send()
        .await
        .expect("unknown-size headers should arrive");
    assert_eq!(response.content_length(), None);
    let first = response
        .chunk()
        .await
        .expect("overflow sentinel chunk should read")
        .expect("overflow sentinel chunk should exist");
    assert!(first.len() > 1024);
    drop(response);
    gate.release();
    client
        .get(server.url("/after"))
        .send()
        .await
        .expect("follow-up should complete")
        .bytes()
        .await
        .expect("follow-up body should complete");
    let records = server.requests();
    assert_eq!(records.len(), 2);
    assert_ne!(records[0].connection_id(), records[1].connection_id());
}

fn child_command(test_name: &str) -> Command {
    let executable = std::env::current_exe().expect("test executable path should exist");
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg(format!("reqwest_contract::{test_name}"))
        .arg("--test-threads=1");
    command
}

fn child_socket(name: &str) -> SocketAddr {
    std::env::var(name)
        .unwrap_or_else(|_| panic!("child variable {name} should exist"))
        .parse()
        .unwrap_or_else(|_| panic!("child variable {name} should contain a socket address"))
}

fn assert_child_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "isolated proxy child failed (status {}, stdout {} bytes, stderr {} bytes)",
        output.status,
        output.stdout.len(),
        output.stderr.len()
    );
}
