//! A transport of Concord with virtual time.
//!
//! No socket: under `#[tokio::test(start_paused = true)]`, time moves only when the test
//! or the engine sleeps, and every wait of the script is a sleep of Tokio's clock.

use bytes::Bytes;
use concord_core::error::Class;
use concord_core::reqwest;
use concord_core::transport::{Transport, TransportError, TryInfo};
use http::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_LENGTH};
use http::{Method, StatusCode};
use hyper::body::{Body, Frame, SizeHint};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::{Instant, Sleep};

/// A [`Transport`] that plays scripted tries, one per call of `send`, in the order they
/// were written, and records each request it was given.
///
/// Clones share the script and the record: give one to the client, keep one to read.
/// A try that finds nothing scripted gets a `599`, so that a test fails loudly.
///
/// It is `Send + Sync`, and works on a multi-thread runtime
/// (`#[tokio::test(flavor = "multi_thread")]`, tasks that share one client): the tries are
/// played in the order the sends reach it, which tasks running at once do not fix, and its
/// waits are real time there, since only a current-thread runtime can pause the clock.
#[derive(Clone, Default)]
pub struct Script {
    shared: Arc<Mutex<State>>,
}

impl std::fmt::Debug for Script {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let state = self.lock();
        f.debug_struct("Script")
            .field("scripted", &state.steps.len())
            .field("requests", &state.requests.len())
            .finish()
    }
}

#[derive(Default)]
struct State {
    steps: VecDeque<Step>,
    requests: Vec<Request>,
}

struct Step {
    /// How the upload is read before the outcome; `None`: it is not read.
    read: Option<Duration>,
    outcome: Outcome,
}

enum Outcome {
    Answer(Spec),
    Fail {
        after: Duration,
        error: TransportError,
    },
    Never,
}

#[derive(Default)]
struct Spec {
    after: Duration,
    status: u16,
    headers: HeaderMap,
    declared: Option<u64>,
    pieces: VecDeque<Piece>,
    end: End,
}

enum Piece {
    Bytes(Bytes),
    Pause(Duration),
}

#[derive(Clone, Copy, Default)]
enum End {
    #[default]
    Complete,
    Stall,
    Cut,
}

impl Script {
    /// An empty script.
    pub fn new() -> Script {
        Script::default()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A panic elsewhere must not hide what was recorded.
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn push(&self, read: Option<Duration>, outcome: Outcome) {
        self.lock().steps.push_back(Step { read, outcome });
    }

    /// The next try gets an answer with this status. It is added when the [`Answer`] is
    /// dropped, at the end of the statement.
    ///
    /// # Panics
    ///
    /// When `status` is not between 100 and 999.
    pub fn answer(&self, status: u16) -> Answer<'_> {
        assert!(
            StatusCode::from_u16(status).is_ok(),
            "concord-testkit: {status} is not a status code (100 to 999)"
        );
        Answer {
            script: self,
            read: None,
            spec: Spec {
                status,
                ..Spec::default()
            },
        }
    }

    /// The next try fails with `error`. It is added when the [`Failure`] is dropped.
    pub fn fail(&self, error: TransportError) -> Failure<'_> {
        Failure {
            script: self,
            read: None,
            after: Duration::ZERO,
            error: Some(error),
        }
    }

    /// The next try never answers. It keeps the request, its upload unread, until the
    /// try is dropped; [`Never::read_upload`] reads it. It is added when the [`Never`] is
    /// dropped, at the end of the statement.
    pub fn never(&self) -> Never<'_> {
        Never {
            script: self,
            read: None,
        }
    }

    /// Every request given to the script, in order, with its try and its virtual instant.
    pub fn requests(&self) -> Vec<Request> {
        self.lock().requests.clone()
    }

    /// How many scripted tries have not been played.
    pub fn unused(&self) -> usize {
        self.lock().steps.len()
    }

    /// Record the request and take the next step; the index of the record.
    fn take(&self, request: &reqwest::Request, info: TryInfo) -> (usize, Option<Step>) {
        let request = Request {
            method: request.method().clone(),
            url: request.url().as_str().to_owned(),
            headers: request.headers().clone(),
            body: request
                .body()
                .and_then(|body| body.as_bytes())
                .map(Bytes::copy_from_slice),
            upload: Upload::default(),
            info,
            at: Instant::now(),
        };
        let mut state = self.lock();
        state.requests.push(request);
        (state.requests.len() - 1, state.steps.pop_front())
    }

    fn update_upload(&self, index: usize, change: impl FnOnce(&mut Upload)) {
        if let Some(request) = self.lock().requests.get_mut(index) {
            change(&mut request.upload);
        }
    }

    /// Read the upload, with `pause` after each piece; the record at `index` follows.
    async fn read_upload(
        &self,
        index: usize,
        body: Option<&mut reqwest::Body>,
        pause: Duration,
    ) -> Result<(), TransportError> {
        self.update_upload(index, |upload| upload.state = UploadState::Reading);
        let Some(body) = body else {
            self.update_upload(index, |upload| upload.state = UploadState::Ended);
            return Ok(());
        };
        let mut body = Pin::new(body);
        loop {
            match std::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
                None => {
                    self.update_upload(index, |upload| upload.state = UploadState::Ended);
                    return Ok(());
                }
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        self.update_upload(index, |upload| {
                            upload.bytes.extend_from_slice(&data);
                            upload.pieces += 1;
                        });
                        wait(pause).await;
                    }
                }
                Some(Err(error)) => {
                    let text = chain(&error);
                    self.update_upload(index, |upload| {
                        upload.state = UploadState::Failed(text);
                    });
                    return Err(TransportError::new(Class::Other).with_source(error));
                }
            }
        }
    }
}

/// An error and its sources, joined by `: `.
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

impl Transport for Script {
    async fn send(
        &self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let try_text = format!("try {} of {}", info.number(), info.endpoint());
        let url = request.url().clone();
        let (index, step) = self.take(&request, info);
        let Some(step) = step else {
            return Ok(respond(
                StatusCode::from_u16(599).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                HeaderMap::new(),
                reqwest::Body::from(format!(
                    "concord-testkit: nothing scripted for {try_text}\n"
                )),
                url,
            ));
        };
        if let Some(pause) = step.read {
            self.read_upload(index, request.body_mut().as_mut(), pause)
                .await?;
        }
        match step.outcome {
            Outcome::Never => {
                // The request, and its upload, are kept until the try is dropped.
                let _kept = request;
                std::future::pending().await
            }
            Outcome::Fail { after, error } => {
                wait(after).await;
                Err(error)
            }
            Outcome::Answer(spec) => {
                wait(spec.after).await;
                let status =
                    StatusCode::from_u16(spec.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
                let body = ScriptBody {
                    pieces: spec.pieces,
                    sleep: None,
                    end: spec.end,
                    declared: spec.declared,
                    given: 0,
                    done: false,
                };
                Ok(respond(
                    status,
                    spec.headers,
                    reqwest::Body::wrap(body),
                    url,
                ))
            }
        }
    }
}

async fn wait(duration: Duration) {
    if !duration.is_zero() {
        tokio::time::sleep(duration).await;
    }
}

/// An answer with the URL of its request, as a real one has.
fn respond(
    status: StatusCode,
    headers: HeaderMap,
    body: reqwest::Body,
    url: reqwest::Url,
) -> reqwest::Response {
    use reqwest::ResponseBuilderExt;
    // A builder with only an extension cannot fail; if it did, the answer has no URL.
    let mut response = http::Response::builder()
        .url(url)
        .body(())
        .unwrap_or_else(|_| http::Response::new(()))
        .map(|()| body);
    *response.status_mut() = status;
    *response.headers_mut() = headers;
    reqwest::Response::from(response)
}

/// An answer being scripted. It is added to the script when dropped, at the end of the
/// statement.
pub struct Answer<'a> {
    script: &'a Script,
    read: Option<Duration>,
    spec: Spec,
}

impl std::fmt::Debug for Answer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Answer")
            .field("status", &self.spec.status)
            .finish_non_exhaustive()
    }
}

impl Drop for Answer<'_> {
    fn drop(&mut self) {
        let spec = std::mem::take(&mut self.spec);
        self.script.push(self.read, Outcome::Answer(spec));
    }
}

impl Answer<'_> {
    /// The head comes after this much virtual time from the hand-over, or from the end
    /// of the upload when it is read ([`Answer::read_upload`]).
    pub fn after(mut self, duration: Duration) -> Self {
        self.spec.after = duration;
        self
    }

    /// Before the head, read the whole body of the request, as fast as it comes. What was
    /// read is recorded ([`Request::upload`]); when the body fails, the try fails with a
    /// [`TransportError`] of class `Other`, whose source is the body's error.
    pub fn read_upload(mut self) -> Self {
        self.read = Some(Duration::ZERO);
        self
    }

    /// As [`Answer::read_upload`], with a pause of `pause` of virtual time after each
    /// piece, before the next is pulled.
    pub fn read_upload_paced(mut self, pause: Duration) -> Self {
        self.read = Some(pause);
        self
    }

    /// A header, added after those before it. A name may repeat. A `content-length` is
    /// a declared length ([`Answer::declare_length`]).
    ///
    /// # Panics
    ///
    /// When the name or the value is not valid in a header, or when a `content-length`
    /// is not a number.
    // PANIC-SAFETY: This test helper documents invalid fixture headers as programmer errors.
    #[allow(clippy::panic)]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        let name = match HeaderName::from_bytes(name.as_bytes()) {
            Ok(name) => name,
            Err(_) => panic!("concord-testkit: {name:?} is not a header name"),
        };
        if name == CONTENT_LENGTH {
            return match value.parse::<u64>() {
                Ok(length) if value.bytes().all(|byte| byte.is_ascii_digit()) => {
                    self.declare_length(length)
                }
                _ => panic!("concord-testkit: content-length {value:?} is not a number"),
            };
        }
        let value = match HeaderValue::from_str(value) {
            Ok(value) => value,
            Err(_) => panic!("concord-testkit: {value:?} is not a header value"),
        };
        self.spec.headers.append(name, value);
        self
    }

    /// The answer says `content-length: length`, as on the wire: the body gives at most
    /// `length` bytes, and when its pieces end before, reading it fails at that early end
    /// (with the error [`Cut`]).
    pub fn declare_length(mut self, length: u64) -> Self {
        self.spec
            .headers
            .insert(CONTENT_LENGTH, HeaderValue::from(length));
        self.spec.declared = Some(length);
        self
    }

    /// A piece of the body. Each piece is one frame; call again for the next.
    pub fn body(mut self, piece: impl Into<Bytes>) -> Self {
        self.spec.pieces.push_back(Piece::Bytes(piece.into()));
        self
    }

    /// A pause of virtual time in the body, before what follows.
    pub fn pause(mut self, duration: Duration) -> Self {
        self.spec.pieces.push_back(Piece::Pause(duration));
        self
    }

    /// After the pieces, the body never ends and never gives another byte.
    pub fn stall(mut self) -> Self {
        self.spec.end = End::Stall;
        self
    }

    /// After the pieces, the body fails, as when a connection is lost in the middle.
    pub fn cut(mut self) -> Self {
        self.spec.end = End::Cut;
        self
    }
}

/// A failure being scripted. It is added to the script when dropped, at the end of the
/// statement.
pub struct Failure<'a> {
    script: &'a Script,
    read: Option<Duration>,
    after: Duration,
    error: Option<TransportError>,
}

impl std::fmt::Debug for Failure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Failure")
            .field("after", &self.after)
            .field("error", &self.error)
            .finish()
    }
}

impl Drop for Failure<'_> {
    fn drop(&mut self) {
        if let Some(error) = self.error.take() {
            self.script.push(
                self.read,
                Outcome::Fail {
                    after: self.after,
                    error,
                },
            );
        }
    }
}

impl Failure<'_> {
    /// The failure comes after this much virtual time from the hand-over, or from the
    /// end of the upload when it is read ([`Failure::read_upload`]).
    pub fn after(mut self, duration: Duration) -> Self {
        self.after = duration;
        self
    }

    /// Before failing, read the whole body of the request, as [`Answer::read_upload`].
    pub fn read_upload(mut self) -> Self {
        self.read = Some(Duration::ZERO);
        self
    }

    /// Before failing, read the body of the request, as [`Answer::read_upload_paced`].
    pub fn read_upload_paced(mut self, pause: Duration) -> Self {
        self.read = Some(pause);
        self
    }
}

/// A try that never answers, being scripted ([`Script::never`]). It is added to the
/// script when dropped, at the end of the statement.
pub struct Never<'a> {
    script: &'a Script,
    read: Option<Duration>,
}

impl std::fmt::Debug for Never<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Never").finish_non_exhaustive()
    }
}

impl Drop for Never<'_> {
    fn drop(&mut self) {
        self.script.push(self.read, Outcome::Never);
    }
}

impl Never<'_> {
    /// Read the whole body of the request, as [`Answer::read_upload`], then never answer.
    pub fn read_upload(mut self) -> Self {
        self.read = Some(Duration::ZERO);
        self
    }

    /// Read the body of the request, as [`Answer::read_upload_paced`], then never answer.
    pub fn read_upload_paced(mut self, pause: Duration) -> Self {
        self.read = Some(pause);
        self
    }
}

/// A request as a [`Script`] was given it.
#[derive(Clone, Debug)]
pub struct Request {
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Option<Bytes>,
    upload: Upload,
    info: TryInfo,
    at: Instant,
}

impl Request {
    /// Borrow the HTTP method recorded when the request reached the transport.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// The whole URL.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Borrow all recorded header values, including repeated fields and sensitivity flags.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// The first value of a header. `None` when absent or not UTF-8.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name)?.to_str().ok()
    }

    /// The body, when it is bytes in memory. `None` without a body or for a stream: see
    /// [`Request::upload`] for what was read of a stream.
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }

    /// What the script read of the body ([`Answer::read_upload`]).
    pub fn upload(&self) -> &Upload {
        &self.upload
    }

    /// The endpoint and the number of the try.
    pub fn info(&self) -> &TryInfo {
        &self.info
    }

    /// The virtual instant at which the request was handed over.
    pub fn at(&self) -> Instant {
        self.at
    }
}

/// What a [`Script`] read of the body of a request.
#[derive(Clone, Debug, Default)]
pub struct Upload {
    bytes: Vec<u8>,
    pieces: usize,
    state: UploadState,
}

impl Upload {
    /// The bytes read, in order.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many pieces of data were read.
    pub fn pieces(&self) -> usize {
        self.pieces
    }

    /// Whether the body was read, and how it ended.
    pub fn state(&self) -> &UploadState {
        &self.state
    }
}

/// How the reading of an upload stands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UploadState {
    /// The script was not told to read it.
    #[default]
    Unread,
    /// Being read: the body has not ended, or the try was dropped while it was read.
    Reading,
    /// Read to its end (a request without a body ends at once).
    Ended,
    /// The body failed; its error and sources, joined by `: `.
    Failed(String),
}

/// The error of a body that [`Answer::cut`] ends, or that ends before its declared
/// length ([`Answer::declare_length`]).
#[derive(Debug)]
pub struct Cut;

impl std::fmt::Display for Cut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("concord-testkit: the scripted body was cut")
    }
}

impl std::error::Error for Cut {}

/// The body of a scripted answer: pieces and pauses of virtual time.
struct ScriptBody {
    pieces: VecDeque<Piece>,
    // In a mutex only to be `Sync`, as `reqwest::Body::wrap` asks.
    sleep: Option<Mutex<Pin<Box<Sleep>>>>,
    end: End,
    declared: Option<u64>,
    /// Bytes given so far.
    given: u64,
    /// The body has ended or failed: nothing more.
    done: bool,
}

impl ScriptBody {
    fn met_declared(&self) -> bool {
        self.declared.is_some_and(|declared| self.given >= declared)
    }
}

impl Body for ScriptBody {
    type Data = Bytes;
    type Error = Cut;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Cut>>> {
        let this = self.get_mut();
        loop {
            if this.done || this.met_declared() {
                this.done = true;
                return Poll::Ready(None);
            }
            if let Some(sleep) = this.sleep.as_mut() {
                let sleep = sleep
                    .get_mut()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if sleep.as_mut().poll(cx).is_pending() {
                    return Poll::Pending;
                }
                this.sleep = None;
            }
            match this.pieces.pop_front() {
                Some(Piece::Bytes(mut bytes)) => {
                    if let Some(declared) = this.declared {
                        let left = declared.saturating_sub(this.given);
                        if (bytes.len() as u64) > left {
                            bytes.truncate(usize::try_from(left).unwrap_or(usize::MAX));
                        }
                    }
                    this.given = this.given.saturating_add(bytes.len() as u64);
                    return Poll::Ready(Some(Ok(Frame::data(bytes))));
                }
                Some(Piece::Pause(duration)) => {
                    this.sleep = Some(Mutex::new(Box::pin(tokio::time::sleep(duration))));
                }
                None => {
                    return match this.end {
                        // Never woken: the body waits for ever.
                        End::Stall => Poll::Pending,
                        // The pieces ended before the declared length: on the wire, the
                        // connection closed early.
                        End::Complete if this.declared.is_some() => {
                            this.done = true;
                            Poll::Ready(Some(Err(Cut)))
                        }
                        End::Complete => {
                            this.done = true;
                            Poll::Ready(None)
                        }
                        End::Cut => {
                            this.done = true;
                            Poll::Ready(Some(Err(Cut)))
                        }
                    };
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.done
            || self.met_declared()
            || (matches!(self.end, End::Complete)
                && self.declared.is_none()
                && self.pieces.is_empty()
                && self.sleep.is_none())
    }

    fn size_hint(&self) -> SizeHint {
        if let Some(declared) = self.declared {
            // What the head announced, as a client reads it from the wire.
            return SizeHint::with_exact(declared.saturating_sub(self.given));
        }
        let size: u64 = self
            .pieces
            .iter()
            .map(|piece| match piece {
                Piece::Bytes(bytes) => bytes.len() as u64,
                Piece::Pause(_) => 0,
            })
            .sum();
        match self.end {
            End::Complete => SizeHint::with_exact(size),
            End::Stall | End::Cut => {
                let mut hint = SizeHint::new();
                hint.set_lower(size);
                hint
            }
        }
    }
}
