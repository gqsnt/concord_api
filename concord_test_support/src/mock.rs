#![allow(dead_code)]

use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[derive(Clone)]
pub struct RecordedRequest {
    pub method: Method,
    pub url: url::Url,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub endpoint: Option<String>,
    pub page_index: Option<u32>,
    pub timeout: Option<Duration>,
    body_complete: bool,
}

impl std::fmt::Debug for RecordedRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RecordedRequest")
            .field("method", &self.method)
            .field("url", &"<redacted>")
            .field(
                "headers",
                &concord_core::advanced::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &format_args!("<{} bytes>", self.body.len()))
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct MockReply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    disconnect: bool,
    response_steps: Option<Vec<ResponseStep>>,
    delay: Option<Duration>,
    gate: Option<ReplyGate>,
    expect_request_body_failure: bool,
}

#[derive(Clone, Debug)]
pub enum ResponseStep {
    Chunk(Bytes),
    Gate(ReplyGate),
    Disconnect,
}

#[derive(Clone, Debug, Default)]
pub struct ReplyGate {
    state: Arc<(Mutex<ReplyGateState>, Condvar)>,
}

#[derive(Debug, Default)]
struct ReplyGateState {
    entered: bool,
    released: bool,
}

impl ReplyGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn wait_until_entered(&self, timeout: Duration) {
        let (lock, condition) = &*self.state;
        let state = lock.lock().expect("reply gate lock");
        let (state, result) = condition
            .wait_timeout_while(state, timeout, |state| !state.entered)
            .expect("reply gate wait");
        assert!(
            state.entered && !result.timed_out(),
            "reply gate was not entered"
        );
    }

    pub fn release(&self) {
        let (lock, condition) = &*self.state;
        lock.lock().expect("reply gate lock").released = true;
        condition.notify_all();
    }

    fn release_for_shutdown(&self) {
        let (lock, condition) = &*self.state;
        if let Ok(mut state) = lock.lock() {
            state.released = true;
        }
        condition.notify_all();
    }

    fn wait(&self, stop: &AtomicBool) {
        let (lock, condition) = &*self.state;
        let mut state = lock.lock().expect("reply gate lock");
        state.entered = true;
        condition.notify_all();
        while !state.released && !stop.load(Ordering::Acquire) {
            (state, _) = condition
                .wait_timeout(state, Duration::from_millis(10))
                .expect("reply gate wait");
        }
    }
}

impl MockReply {
    pub fn ok_json(body: Bytes) -> Self {
        Self::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("application/json"),
            )
            .with_body(body)
    }

    pub fn ok_text(body: Bytes) -> Self {
        Self::status(StatusCode::OK)
            .with_header(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/plain"),
            )
            .with_body(body)
    }

    pub fn status(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
            disconnect: false,
            response_steps: None,
            delay: None,
            gate: None,
            expect_request_body_failure: false,
        }
    }

    pub fn disconnect_after_request() -> Self {
        Self {
            disconnect: true,
            ..Self::status(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }

    pub fn with_header(mut self, name: http::HeaderName, value: http::HeaderValue) -> Self {
        self.headers.insert(name, value);
        self
    }

    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = body;
        self
    }

    pub fn with_chunks(mut self, chunks: impl IntoIterator<Item = Bytes>) -> Self {
        self.response_steps = Some(chunks.into_iter().map(ResponseStep::Chunk).collect());
        self
    }

    pub fn with_response_steps(mut self, steps: impl IntoIterator<Item = ResponseStep>) -> Self {
        self.response_steps = Some(steps.into_iter().collect());
        self
    }

    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    pub fn with_gate(mut self, gate: ReplyGate) -> Self {
        self.gate = Some(gate);
        self
    }

    pub fn expect_request_body_failure(mut self) -> Self {
        self.expect_request_body_failure = true;
        self
    }
}

#[derive(Debug)]
struct MockState {
    recorded: Mutex<Vec<RecordedRequest>>,
    replies: Mutex<VecDeque<MockReply>>,
    repeat: Option<MockReply>,
    failure: Mutex<Option<String>>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
    workers: Mutex<Vec<std::thread::JoinHandle<()>>>,
    stop: AtomicBool,
    gates: Vec<ReplyGate>,
    request_head_observer: Option<RequestObserver>,
    request_body_complete_observer: Option<RequestObserver>,
}

#[derive(Clone)]
struct RequestObserver(Arc<dyn Fn() + Send + Sync>);

impl std::fmt::Debug for RequestObserver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RequestObserver(..)")
    }
}

#[derive(Clone, Debug)]
pub struct MockServer {
    base_url: url::Url,
    lifetime: Arc<MockLifetime>,
}

impl MockServer {
    pub fn base_url(&self) -> &url::Url {
        &self.base_url
    }

    pub fn configure_reqwest(
        &self,
        builder: concord_core::advanced::SafeReqwestBuilder,
    ) -> concord_core::advanced::SafeReqwestBuilder {
        #[cfg(feature = "dangerous-dev-tools")]
        let proxy = concord_core::advanced::SafeProxy::__test_origin_override_with_guard(
            self.base_url.as_str(),
            self.lifetime.clone(),
        )
        .expect("loopback mock URL is a safe test origin");
        #[cfg(not(feature = "dangerous-dev-tools"))]
        let proxy = concord_core::advanced::SafeProxy::all(self.base_url.as_str())
            .expect("loopback mock URL is a safe test proxy");
        builder.proxy(proxy)
    }
}

pub struct MockHandle {
    lifetime: Arc<MockLifetime>,
    finished: bool,
}

#[derive(Debug)]
struct MockLifetime {
    state: Arc<MockState>,
}

impl Drop for MockLifetime {
    fn drop(&mut self) {
        signal_shutdown(&self.state);
        if !std::thread::panicking() {
            join_state(&self.state);
        }
    }
}

#[derive(Default)]
pub struct MockBuilder {
    replies: Vec<MockReply>,
    repeat: Option<MockReply>,
    request_head_observer: Option<RequestObserver>,
    request_body_complete_observer: Option<RequestObserver>,
}

impl MockBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(mut self, reply: MockReply) -> Self {
        self.replies.push(reply);
        self
    }

    pub fn replies(mut self, replies: impl IntoIterator<Item = MockReply>) -> Self {
        self.replies.extend(replies);
        self
    }

    pub fn repeating(mut self, reply: MockReply) -> Self {
        self.repeat = Some(reply);
        self
    }

    pub fn on_request_head(mut self, observer: impl Fn() + Send + Sync + 'static) -> Self {
        self.request_head_observer = Some(RequestObserver(Arc::new(observer)));
        self
    }

    pub fn on_request_body_complete(mut self, observer: impl Fn() + Send + Sync + 'static) -> Self {
        self.request_body_complete_observer = Some(RequestObserver(Arc::new(observer)));
        self
    }

    pub fn build(self) -> (MockServer, MockHandle) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("mock server bind");
        listener
            .set_nonblocking(true)
            .expect("mock server nonblocking");
        let address = listener.local_addr().expect("mock server address");
        let base_url = url::Url::parse(&format!("http://{address}/")).expect("mock server URL");
        let gates = self
            .replies
            .iter()
            .chain(self.repeat.iter())
            .flat_map(reply_gates)
            .collect();
        let state = Arc::new(MockState {
            recorded: Mutex::new(Vec::new()),
            replies: Mutex::new(self.replies.into_iter().collect()),
            repeat: self.repeat,
            failure: Mutex::new(None),
            thread: Mutex::new(None),
            workers: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            gates,
            request_head_observer: self.request_head_observer,
            request_body_complete_observer: self.request_body_complete_observer,
        });
        let thread_state = state.clone();
        let base = base_url.clone();
        let thread = std::thread::spawn(move || {
            while !thread_state.stop.load(Ordering::Acquire) {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1));
                        continue;
                    }
                    Err(error) => {
                        *thread_state.failure.lock().expect("mock failure lock") =
                            Some(format!("mock accept failed: {error}"));
                        return;
                    }
                };
                if let Err(error) = stream.set_nonblocking(false) {
                    *thread_state.failure.lock().expect("mock failure lock") =
                        Some(format!("mock accepted socket setup failed: {error}"));
                    return;
                }
                let reply = thread_state
                    .replies
                    .lock()
                    .expect("mock replies lock")
                    .pop_front()
                    .or_else(|| thread_state.repeat.clone());
                let Some(reply) = reply else {
                    *thread_state.failure.lock().expect("mock failure lock") =
                        Some("unexpected request exceeded scripted replies".to_string());
                    return;
                };
                let request_state = thread_state.clone();
                let request_base = base.clone();
                let worker = std::thread::spawn(move || {
                    if let Err(error) = serve_one(stream, &request_base, reply, &request_state) {
                        *request_state.failure.lock().expect("mock failure lock") =
                            Some(format!("mock request handling failed: {error}"));
                    }
                });
                thread_state
                    .workers
                    .lock()
                    .expect("mock workers lock")
                    .push(worker);
            }
        });
        *state.thread.lock().expect("mock thread lock") = Some(thread);
        let lifetime = Arc::new(MockLifetime { state });
        (
            MockServer {
                base_url,
                lifetime: lifetime.clone(),
            },
            MockHandle {
                lifetime,
                finished: false,
            },
        )
    }
}

pub fn mock() -> MockBuilder {
    MockBuilder::new()
}

impl MockHandle {
    pub fn recorded(&self) -> Vec<RecordedRequest> {
        self.lifetime
            .state
            .recorded
            .lock()
            .expect("mock recorded lock")
            .clone()
    }

    pub fn recorded_len(&self) -> usize {
        self.lifetime
            .state
            .recorded
            .lock()
            .expect("mock recorded lock")
            .len()
    }

    /// Number of physical HTTP requests observed on the loopback wire.
    ///
    /// This deliberately differs from a Concord visible-execution count:
    /// Reqwest-internal retries increase this value without rerunning Concord
    /// hooks or rate-limit acquisition.
    pub fn wire_request_count(&self) -> usize {
        self.recorded_len()
    }

    pub fn completed_len(&self) -> usize {
        self.lifetime
            .state
            .recorded
            .lock()
            .expect("mock recorded lock")
            .iter()
            .filter(|request| request.body_complete || !request.body.is_empty())
            .count()
    }

    pub fn assert_recorded_len(&self, expected: usize) {
        assert_eq!(self.recorded_len(), expected, "recorded request count");
    }

    pub fn finish(mut self) {
        self.join();
        self.finished = true;
    }

    fn join(&self) {
        join_state(&self.lifetime.state);
    }
}

impl Drop for MockHandle {
    fn drop(&mut self) {
        if !self.finished {
            if std::thread::panicking() {
                signal_shutdown(&self.lifetime.state);
            } else if Arc::strong_count(&self.lifetime) == 1 {
                self.join();
            }
        }
    }
}

fn reply_gates(reply: &MockReply) -> Vec<ReplyGate> {
    let mut gates = Vec::new();
    if let Some(gate) = &reply.gate {
        gates.push(gate.clone());
    }
    if let Some(steps) = &reply.response_steps {
        gates.extend(steps.iter().filter_map(|step| match step {
            ResponseStep::Gate(gate) => Some(gate.clone()),
            ResponseStep::Chunk(_) | ResponseStep::Disconnect => None,
        }));
    }
    gates
}

fn signal_shutdown(state: &MockState) {
    state.stop.store(true, Ordering::Release);
    for gate in &state.gates {
        gate.release_for_shutdown();
    }
}

fn join_state(state: &MockState) {
    signal_shutdown(state);
    if let Some(thread) = state.thread.lock().expect("mock thread lock").take() {
        thread.join().expect("mock server thread");
    }
    for worker in state.workers.lock().expect("mock workers lock").drain(..) {
        worker.join().expect("mock request worker");
    }
    if let Some(error) = state.failure.lock().expect("mock failure lock").take() {
        panic!("mock server failed: {error}");
    }
    let unused = state.replies.lock().expect("mock replies lock").len();
    assert_eq!(unused, 0, "mock scripted replies remain unused");
}

fn serve_one(
    stream: TcpStream,
    base: &url::Url,
    reply: MockReply,
    state: &Arc<MockState>,
) -> Result<(), String> {
    if reply.expect_request_body_failure {
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|error| error.to_string())?;
    }
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    if let Err(error) = reader.read_line(&mut request_line) {
        if reply.expect_request_body_failure {
            return Ok(());
        }
        return Err(error.to_string());
    }
    if request_line.is_empty() && reply.expect_request_body_failure {
        return Ok(());
    }
    let mut words = request_line.split_whitespace();
    let method = words
        .next()
        .ok_or_else(|| "missing request method".to_string())?
        .parse::<Method>()
        .map_err(|error| error.to_string())?;
    let target = words
        .next()
        .ok_or_else(|| "missing request target".to_string())?;
    let mut headers = HeaderMap::new();
    loop {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if line == "\r\n" || line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| "malformed request header".to_string())?;
        headers.append(
            name.parse::<http::HeaderName>()
                .map_err(|error| error.to_string())?,
            value
                .trim()
                .parse::<http::HeaderValue>()
                .map_err(|error| error.to_string())?,
        );
    }
    if let Some(observer) = &state.request_head_observer {
        (observer.0)();
    }
    let chunked = headers
        .get(http::header::TRANSFER_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"));
    let content_length = headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let original_url_header = http::HeaderName::from_static("x-concord-test-original-url");
    let url = headers
        .remove(&original_url_header)
        .and_then(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| url::Url::parse(value).ok())
        })
        .unwrap_or_else(|| {
            base.join(target.trim_start_matches('/'))
                .expect("captured request target")
        });
    let endpoint = take_header_string(&mut headers, "x-concord-test-endpoint");
    let page_index = take_header_string(&mut headers, "x-concord-test-page-index")
        .and_then(|value| value.parse().ok());
    let timeout = take_header_string(&mut headers, "x-concord-test-timeout-ms")
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_millis);
    let recorded_index = {
        let mut recorded = state.recorded.lock().expect("mock recorded lock");
        let index = recorded.len();
        recorded.push(RecordedRequest {
            method,
            url,
            headers,
            body: Bytes::new(),
            endpoint,
            page_index,
            timeout,
            body_complete: false,
        });
        index
    };

    let (body, incomplete_body) = if chunked {
        read_chunked(&mut reader, state, recorded_index)
    } else {
        read_sized_body(&mut reader, content_length, state, recorded_index)
    };
    if let Some(recorded) = state
        .recorded
        .lock()
        .expect("mock recorded lock")
        .get_mut(recorded_index)
    {
        recorded.body = Bytes::from(body);
        recorded.body_complete = true;
    }
    if let Some(observer) = &state.request_body_complete_observer {
        (observer.0)();
    }
    if let Some(error) = incomplete_body.as_ref()
        && !reply.expect_request_body_failure
    {
        return Err(error.clone());
    }
    if incomplete_body.is_some() && reply.expect_request_body_failure {
        return Ok(());
    }
    if let Some(delay) = reply.delay {
        std::thread::sleep(delay);
    }
    if let Some(gate) = &reply.gate {
        gate.wait(&state.stop);
    }
    if reply.disconnect {
        return Ok(());
    }
    let stream = reader.get_mut();
    write!(
        stream,
        "HTTP/1.1 {} {}\r\n",
        reply.status.as_u16(),
        reason(reply.status)
    )
    .map_err(|error| error.to_string())?;
    for (name, value) in &reply.headers {
        write!(stream, "{}: {}\r\n", name, value.to_str().unwrap_or(""))
            .map_err(|error| error.to_string())?;
    }
    if let Some(steps) = reply.response_steps {
        write!(
            stream,
            "Transfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
        )
        .map_err(|error| error.to_string())?;
        for step in steps {
            let chunk = match step {
                ResponseStep::Chunk(chunk) => chunk,
                ResponseStep::Gate(gate) => {
                    gate.wait(&state.stop);
                    continue;
                }
                ResponseStep::Disconnect => return Ok(()),
            };
            if state.stop.load(Ordering::Acquire) {
                return Ok(());
            }
            write!(stream, "{:x}\r\n", chunk.len()).map_err(|error| error.to_string())?;
            stream
                .write_all(&chunk)
                .map_err(|error| error.to_string())?;
            stream
                .write_all(b"\r\n")
                .map_err(|error| error.to_string())?;
        }
        stream
            .write_all(b"0\r\n\r\n")
            .map_err(|error| error.to_string())
    } else {
        if !reply.headers.contains_key(http::header::CONTENT_LENGTH) {
            write!(stream, "Content-Length: {}\r\n", reply.body.len())
                .map_err(|error| error.to_string())?;
        }
        write!(stream, "Connection: close\r\n\r\n").map_err(|error| error.to_string())?;
        stream
            .write_all(&reply.body)
            .map_err(|error| error.to_string())
    }
}

fn take_header_string(headers: &mut HeaderMap, name: &'static str) -> Option<String> {
    headers
        .remove(http::HeaderName::from_static(name))
        .and_then(|value| value.to_str().ok().map(str::to_owned))
}

fn update_recorded_body(state: &MockState, recorded_index: usize, body: &[u8]) {
    if let Some(recorded) = state
        .recorded
        .lock()
        .expect("mock recorded lock")
        .get_mut(recorded_index)
    {
        recorded.body = Bytes::copy_from_slice(body);
    }
}

fn read_chunked(
    reader: &mut BufReader<TcpStream>,
    state: &MockState,
    recorded_index: usize,
) -> (Vec<u8>, Option<String>) {
    let mut body = Vec::new();
    loop {
        let mut size = String::new();
        if let Err(error) = reader.read_line(&mut size) {
            return (body, Some(error.to_string()));
        }
        let size = match usize::from_str_radix(size.trim().split(';').next().unwrap_or(""), 16) {
            Ok(size) => size,
            Err(error) => return (body, Some(error.to_string())),
        };
        if size == 0 {
            let mut end = String::new();
            if let Err(error) = reader.read_line(&mut end) {
                return (body, Some(error.to_string()));
            }
            break;
        }
        let start = body.len();
        body.resize(start + size, 0);
        if let Err(error) = reader.read_exact(&mut body[start..]) {
            body.truncate(start);
            return (body, Some(error.to_string()));
        }
        update_recorded_body(state, recorded_index, &body);
        let mut crlf = [0; 2];
        if let Err(error) = reader.read_exact(&mut crlf) {
            return (body, Some(error.to_string()));
        }
    }
    (body, None)
}

fn read_sized_body(
    reader: &mut BufReader<TcpStream>,
    length: usize,
    state: &MockState,
    recorded_index: usize,
) -> (Vec<u8>, Option<String>) {
    let mut body = Vec::with_capacity(length);
    while body.len() < length {
        let mut chunk = vec![0; (length - body.len()).min(8 * 1024)];
        match reader.read(&mut chunk) {
            Ok(0) => {
                return (
                    body,
                    Some("request body ended before Content-Length".to_string()),
                );
            }
            Ok(read) => {
                body.extend_from_slice(&chunk[..read]);
                update_recorded_body(state, recorded_index, &body);
            }
            Err(error) => return (body, Some(error.to_string())),
        }
    }
    (body, None)
}

fn reason(status: StatusCode) -> &'static str {
    status.canonical_reason().unwrap_or("Response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "mock scripted replies remain unused")]
    fn finish_rejects_unused_scripted_replies() {
        let (_server, handle) = mock().reply(MockReply::status(StatusCode::OK)).build();
        handle.finish();
    }

    #[test]
    #[should_panic(expected = "mock scripted replies remain unused")]
    fn final_nonpanicking_drop_rejects_unused_scripted_replies() {
        let (server, handle) = mock().reply(MockReply::status(StatusCode::OK)).build();
        drop(server);
        drop(handle);
    }

    #[test]
    #[should_panic(expected = "unexpected request exceeded scripted replies")]
    fn finish_rejects_unexpected_requests() {
        let (server, handle) = mock().build();
        let mut stream = TcpStream::connect(
            server
                .base_url()
                .socket_addrs(|| None)
                .expect("socket address")[0],
        )
        .expect("connect");
        stream
            .write_all(b"GET /unexpected HTTP/1.1\r\nHost: example.test\r\n\r\n")
            .expect("request");
        std::thread::sleep(Duration::from_millis(10));
        handle.finish();
    }

    #[test]
    #[should_panic(expected = "mock request handling failed")]
    fn finish_rejects_malformed_requests() {
        let (server, handle) = mock().reply(MockReply::status(StatusCode::OK)).build();
        let mut stream = TcpStream::connect(
            server
                .base_url()
                .socket_addrs(|| None)
                .expect("socket address")[0],
        )
        .expect("connect");
        stream.write_all(b"malformed\r\n\r\n").expect("request");
        drop(stream);
        std::thread::sleep(Duration::from_millis(10));
        handle.finish();
    }

    #[test]
    fn gated_reply_records_before_release_and_finishes_cleanly() {
        let gate = ReplyGate::new();
        let (server, handle) = mock()
            .reply(MockReply::status(StatusCode::OK).with_gate(gate.clone()))
            .build();
        let address = server.base_url().socket_addrs(|| None).expect("address")[0];
        let request = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect");
            stream
                .write_all(b"GET /gated HTTP/1.1\r\nHost: example.test\r\n\r\n")
                .expect("request");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("response");
            response
        });

        gate.wait_until_entered(Duration::from_secs(1));
        handle.assert_recorded_len(1);
        gate.release();
        assert!(
            request
                .join()
                .expect("request thread")
                .starts_with("HTTP/1.1 200")
        );
        handle.finish();
    }

    #[test]
    fn request_head_and_body_completion_are_distinct_events() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let head_events = events.clone();
        let body_events = events.clone();
        let (server, handle) = mock()
            .reply(MockReply::status(StatusCode::OK))
            .on_request_head(move || {
                head_events.lock().expect("events").push("head");
            })
            .on_request_body_complete(move || {
                body_events.lock().expect("events").push("body-complete");
            })
            .build();
        let address = server.base_url().socket_addrs(|| None).expect("address")[0];
        let mut stream = TcpStream::connect(address).expect("connect");
        stream
            .write_all(b"POST /phases HTTP/1.1\r\nHost: example.test\r\nContent-Length: 3\r\n\r\n")
            .expect("request head");

        wait_until(Duration::from_secs(1), || {
            events.lock().expect("events").as_slice() == ["head"]
        });
        stream.write_all(b"abc").expect("request body");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("response");

        assert_eq!(
            events.lock().expect("events").as_slice(),
            ["head", "body-complete"]
        );
        handle.finish();
    }

    #[test]
    fn response_step_gate_separates_chunks_and_cleanup_does_not_need_release() {
        let gate = ReplyGate::new();
        let (server, handle) = mock()
            .reply(MockReply::status(StatusCode::OK).with_response_steps([
                ResponseStep::Chunk(Bytes::from_static(b"first")),
                ResponseStep::Gate(gate.clone()),
                ResponseStep::Chunk(Bytes::from_static(b"second")),
            ]))
            .build();
        let address = server.base_url().socket_addrs(|| None).expect("address")[0];
        let mut stream = TcpStream::connect(address).expect("connect");
        stream
            .write_all(b"GET /chunks HTTP/1.1\r\nHost: example.test\r\n\r\n")
            .expect("request");
        gate.wait_until_entered(Duration::from_secs(1));

        // Closing the consumer and finishing the server must release a gate
        // without requiring the test to perform a success-path release.
        drop(stream);
        handle.finish();
    }

    #[test]
    fn captured_endpoint_metadata_is_owned_wire_data() {
        let (server, handle) = mock().reply(MockReply::status(StatusCode::OK)).build();
        let address = server.base_url().socket_addrs(|| None).expect("address")[0];
        let mut stream = TcpStream::connect(address).expect("connect");
        stream
            .write_all(
                b"GET /owned HTTP/1.1\r\nHost: example.test\r\nX-Concord-Test-Endpoint: OwnedEndpoint\r\n\r\n",
            )
            .expect("request");
        let mut response = String::new();
        stream.read_to_string(&mut response).expect("response");

        let requests = handle.recorded();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].endpoint.as_deref(), Some("OwnedEndpoint"));
        let owned = requests[0].endpoint.clone().expect("owned endpoint");
        drop(requests);
        assert_eq!(owned, "OwnedEndpoint");
        handle.finish();
    }

    fn wait_until(timeout: Duration, predicate: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + timeout;
        while !predicate() {
            assert!(std::time::Instant::now() < deadline, "condition timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
