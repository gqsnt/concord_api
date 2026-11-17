use super::{Body, BodyStream, Data};
use bytes::Bytes;
use futures_core::Stream;
use std::{
    collections::VecDeque,
    error::Error,
    fmt,
    future::Future,
    io,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll},
};

type BoxError = Box<dyn Error + Send + Sync>;
type Opening = Pin<Box<dyn Future<Output = Result<BodyStream, BoxError>> + Send>>;
pub(super) type Factory = dyn Fn() -> Opening + Send + Sync;
type Map = Arc<dyn Fn(BodyStream) -> BodyStream + Send + Sync>;

impl Body {
    /// A body that opens a fresh stream for each authorized attempt.
    ///
    /// `make` must return the same immutable bytes from byte zero with an independent
    /// cursor each time. Cloned calls may open concurrently. A path alone does not
    /// make a file immutable: keep its snapshot unchanged until all calls finish.
    /// The first body poll calls `make` and polls its future; construction, cloning,
    /// inspection and dropping an unpolled body do not open it. Author code must
    /// return promptly, without blocking.
    ///
    /// Repeatability makes another attempt possible, not allowed: the retry policy,
    /// method, `idempotent` mark and listed refusal statuses still decide. A retry
    /// starts at byte zero, including after a half-sent upload; it never resumes.
    /// Each edit runs once against an unreadable placeholder body. Removing it or
    /// replacing it with a different kind of body is a Build error. An edit must
    /// leave the body alone: wrappers and look-alikes cannot always be detected.
    /// Reading the placeholder is a Build error.
    /// A replacement body's `size_hint` and `Drop` can run during building;
    /// that code may panic or block and is not guarded by Concord. If building
    /// succeeds, the engine sends the declared repeatable body regardless of the edit.
    /// Middleware sees each attempt's actual stream.
    /// `request()` succeeds without opening it; `into_request()` exports one lazy,
    /// noncloneable request, without Concord's retry plan or clocks. You can read
    /// either exported request's body; reading one does not spend the call's source.
    /// Pages with the same head and the same flattened sequence of bytes, factories
    /// and map identities are a loop. Concatenation grouping does not affect comparison.
    /// A pager that swaps in a distinct factory has a different body, even if its
    /// bytes would match; comparing pages never opens a source.
    ///
    /// The description or `edit_request` may supply `content-length`; the final
    /// header after edits is used. A shorter or longer source fails with
    /// `io::ErrorKind::InvalidData`: the final chunk is held until lookahead proves
    /// EOF. Empty chunks are skipped during lookahead. A single chunk that passes
    /// the length is rejected whole; none of that chunk is sent. Nothing is drained.
    /// Declared zero is checked after middleware admission, inside transport clocks,
    /// before transport handover. An opening or reading error ends the attempt as
    /// [`Other`](crate::error::Class::Other), with the author's error itself as
    /// `source()`, whatever the transport reports. After the first error the
    /// body ends, without opening a later part. The standard policy stops at Other;
    /// a custom `when` rule may retry it under the same permission gates, with a
    /// fresh stream starting at byte zero.
    /// Panics in the factory, polling or cleanup become body errors; returned errors
    /// are kept before cleanup. The author's panic hook still runs. Destruction of
    /// a panic payload is guarded once; a second panic inside that guard is outside
    /// containment. An already-running poll finishes before the attempt closes.
    ///
    /// The core's own body failures have an `io::Error` as their source:
    ///
    /// | Failure | `io::ErrorKind` | Text |
    /// |---|---|---|
    /// | Length mismatch | `InvalidData` | `the content-length does not match the body length` |
    /// | Caught panic | `Other` | `the repeatable body panicked` |
    ///
    /// An author's `io::Error` can have the same kind and text; neither identifies
    /// its origin. If that distinction matters, return an error type of your own:
    /// the opening or reading error is then the source unchanged.
    ///
    /// [`Body::concat`] and a multipart body remain repeatable when all parts are
    /// bytes or repeatable; a one-shot part makes the whole one-shot. Multipart's
    /// boundary is chosen once by its writer. The same repeatable source may occur
    /// twice in a concatenation: each occurrence opens its own cursor, in order.
    /// `multipart_mixed` and `multipart_related` likewise accept repeatable parts.
    /// A map runs once per opening and must
    /// not depend on the attempt. No upload is collected and no task is started.
    /// Added memory holds the parts, factory captures, active stream and one held
    /// chunk; the source's own buffering and chunk size remain its responsibility.
    /// Ready-source work uses a cooperative poll budget.
    /// Descent through maps and terminal destruction have no separate work limit:
    /// their work grows with the composition's depth and number of parts, plus
    /// the author's code. Callbacks, polls and destructors must return promptly;
    /// timeouts cannot interrupt them. No separate task performs cleanup.
    ///
    /// At the transport, idle and configured call timeouts cover opening and reading;
    /// chunks sent reset idle. A middleware that reads the body has only the call
    /// timeout, which has no default, so a stalled opener there can wait forever.
    /// A limit opens nothing while waiting. Dropping a call prevents further opening;
    /// bytes already handed to a transport may already have reached the server.
    ///
    /// ```
    /// # use concord_core as concord;
    /// use concord::{Body, bytes::Bytes};
    /// let body = Body::repeatable(|| async {
    ///     Ok::<_, std::io::Error>(futures_util::stream::iter([
    ///         Ok(Bytes::from_static(b"immutable snapshot")),
    ///     ]))
    /// });
    /// assert!(body.as_bytes().is_none());
    /// let first = body.clone().into_stream()?; // still unopened
    /// let second = body.into_stream()?;       // independent cursor
    /// # let _ = (first, second);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    ///
    /// The facade's `Body` documentation shows a complete asynchronous file upload
    /// with a PUT description and returned headers added by `edit_request`.
    #[must_use]
    pub fn repeatable<F, Fut, S, E>(make: F) -> Body
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<S, E>> + Send + 'static,
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: Into<Box<dyn Error + Send + Sync>>,
    {
        let make: Arc<Factory> = Arc::new(move || {
            Box::pin(OpenFuture {
                work: Box::pin(make()),
            })
        });
        Body {
            content_type: None,
            data: Data::Repeatable(make),
        }
    }
}

// Poll a borrow: a returned failure must reach Fresh before a completed
// author future is destroyed. An async `.await` here would drop it too early.
struct OpenFuture<F> {
    work: Pin<Box<F>>,
}
impl<F, S, E> Future for OpenFuture<F>
where
    F: Future<Output = Result<S, E>>,
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: Into<BoxError>,
{
    type Output = Result<BodyStream, BoxError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.work.as_mut().poll(cx).map(|result| {
            result
                .map(|stream| Box::pin(stream.map(|piece| piece.map_err(Into::into))) as BodyStream)
                .map_err(Into::into)
        })
    }
}
fn guarded_call<T>(work: impl FnOnce() -> T) -> Result<T, BoxError> {
    match catch_unwind(AssertUnwindSafe(work)) {
        Ok(value) => Ok(value),
        Err(payload) => {
            // Guard destruction of the author's panic payload once. A second
            // panic inside that guard is outside the containment boundary.
            let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
            Err(Box::new(io::Error::other("the repeatable body panicked")))
        }
    }
}
fn dispose<T>(value: T) -> Result<(), BoxError> {
    guarded_call(|| drop(value))
}
struct Source(Option<BoxError>);
impl Drop for Source {
    fn drop(&mut self) {
        let _ = dispose(self.0.take());
    }
}

// Shared only by one materialization. Inner compositions never reset this budget.
const POLL_WORK: usize = 64;
pub(super) struct Budget(AtomicUsize);
impl Budget {
    pub(super) fn new() -> Arc<Self> {
        Arc::new(Self(AtomicUsize::new(POLL_WORK)))
    }
    fn step(&self, cx: &Context<'_>) -> bool {
        let before = self.0.load(Ordering::Relaxed);
        if before.checked_sub(1).is_some_and(|after| {
            self.0
                .compare_exchange(before, after, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        }) {
            true
        } else {
            cx.waker().wake_by_ref();
            false
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Control {
    failure: Arc<Failure>,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct Failure {
    state: Mutex<Publication>,
    failed: AtomicBool,
    waker: futures_util::task::AtomicWaker,
}
#[derive(Default)]
struct Publication {
    source: Option<Arc<Source>>,
    active: usize,
    closing: bool,
    closed: bool,
}
impl Control {
    fn record(&self, source: BoxError) -> BoxError {
        let source = Arc::new(Source(Some(source)));
        let mut state = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !state.closed && !self.failure.failed.load(Ordering::Acquire) {
            state.source = Some(source.clone());
            self.failure.failed.store(true, Ordering::Release);
        }
        drop(state);
        self.failure.waker.wake();
        Box::new(SharedFailure(source))
    }
    fn failed(&self) -> bool {
        self.failure.failed.load(Ordering::Acquire)
    }
    pub(crate) fn register(&self, cx: &Context<'_>) {
        self.failure.waker.register(cx.waker());
    }
    fn error(&self) -> Option<BoxError> {
        let source = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .source
            .clone();
        source.map(|source| Box::new(SharedFailure(source)) as BoxError)
    }
    pub(crate) fn failure(&self) -> Option<crate::transport::TransportError> {
        self.error().map(crate::transport::TransportError::upload)
    }
    pub(crate) fn take_failure(&self) -> Option<crate::transport::TransportError> {
        let source = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .source
            .take();
        source.map(|source| crate::transport::TransportError::upload(SharedFailure(source)))
    }
    fn enter(&self) -> Option<Polling> {
        let mut state = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closing {
            return None;
        }
        state.active = state.active.checked_add(1)?;
        Some(Polling(self.clone()))
    }
    pub(crate) fn poll_close(
        &self,
        cx: &Context<'_>,
    ) -> Poll<Option<crate::transport::TransportError>> {
        self.register(cx);
        let mut state = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closing = true;
        self.cancelled.store(true, Ordering::Release);
        if state.active != 0 {
            return Poll::Pending;
        }
        // No new poll can enter, and every admitted poll has published its result.
        state.closed = true;
        drop(state);
        Poll::Ready(self.take_failure())
    }
    fn cancel(&self) {
        let mut state = self
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closing = true;
        self.cancelled.store(true, Ordering::Release);
    }
    pub(crate) fn cancel_on_drop(&self) -> Cancel {
        Cancel(self.clone())
    }
}
struct Polling(Control);
impl Drop for Polling {
    fn drop(&mut self) {
        let mut state = self
            .0
            .failure
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // PANIC-SAFETY: Saturation is defensive; each admitted poll leaves exactly once.
        state.active = state.active.saturating_sub(1);
        let closing = state.closing;
        drop(state);
        if closing {
            self.0.failure.waker.wake();
        }
    }
}
pub(crate) struct Cancel(Control);
impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.cancel();
        self.0.failure.waker.take();
    }
}
struct SharedFailure(Arc<Source>);
impl fmt::Debug for SharedFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestBodyFailure { .. }")
    }
}
impl fmt::Display for SharedFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the request body failed")
    }
}
impl Error for SharedFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.0
             .0
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

// Publication shares ownership; the public error borrows the original cause
// directly instead of exposing that internal ownership wrapper.
pub(crate) fn original_source<'a>(
    source: &'a (dyn Error + Send + Sync + 'static),
) -> &'a (dyn Error + 'static) {
    source
        .downcast_ref::<SharedFailure>()
        .and_then(|shared| shared.0 .0.as_deref())
        .map(|source| source as &(dyn Error + 'static))
        .unwrap_or(source)
}

enum State {
    Start(Arc<Factory>),
    Opening(Opening),
    Reading(BodyStream),
    Done,
}
struct Fresh {
    state: State,
    budget: Option<Arc<Budget>>,
    control: Control,
}
pub(super) fn opening(
    make: Arc<Factory>,
    budget: Option<Arc<Budget>>,
    control: Control,
) -> BodyStream {
    Box::pin(Fresh {
        state: State::Start(make),
        budget,
        control,
    })
}
impl Fresh {
    fn replace(&mut self, state: State) -> Result<(), BoxError> {
        dispose(std::mem::replace(&mut self.state, state))
    }
    fn fail(&mut self, source: BoxError) -> Poll<Option<Result<Bytes, BoxError>>> {
        let source = self.control.record(source);
        let _ = self.replace(State::Done);
        Poll::Ready(Some(Err(source)))
    }
}
impl Drop for Fresh {
    fn drop(&mut self) {
        let _ = self.replace(State::Done);
    }
}
impl Stream for Fresh {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(_polling) = this.control.enter() else {
            let _ = this.replace(State::Done);
            return Poll::Ready(None);
        };
        loop {
            if this.control.failed() || this.control.cancelled.load(Ordering::Acquire) {
                let _ = this.replace(State::Done);
                return Poll::Ready(None);
            }
            if this.budget.as_ref().is_some_and(|budget| !budget.step(cx)) {
                return Poll::Pending;
            }
            let state = match &mut this.state {
                State::Start(make) => match guarded_call(|| make()) {
                    Ok(opening) => State::Opening(opening),
                    Err(source) => return this.fail(source),
                },
                State::Opening(opening) => match guarded_call(|| opening.as_mut().poll(cx)) {
                    Ok(Poll::Pending) => return Poll::Pending,
                    Ok(Poll::Ready(Ok(stream))) => State::Reading(stream),
                    Ok(Poll::Ready(Err(source))) | Err(source) => return this.fail(source),
                },
                State::Reading(stream) => match guarded_call(|| stream.as_mut().poll_next(cx)) {
                    Ok(Poll::Ready(Some(Err(source)))) | Err(source) => return this.fail(source),
                    Ok(Poll::Ready(None)) => State::Done,
                    Ok(next) => return next,
                },
                State::Done => return Poll::Ready(None),
            };
            if let Err(source) = this.replace(state) {
                return this.fail(source);
            }
        }
    }
}
struct Joined {
    parts: VecDeque<BodyStream>,
    budget: Arc<Budget>,
    control: Control,
}
pub(super) fn concatenated(
    parts: VecDeque<BodyStream>,
    budget: Arc<Budget>,
    control: Control,
) -> BodyStream {
    Box::pin(Joined {
        parts,
        budget,
        control,
    })
}
impl Stream for Joined {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.control.failed() || this.control.cancelled.load(Ordering::Acquire) {
            this.parts.clear();
            return Poll::Ready(None);
        }
        while let Some(stream) = this.parts.front_mut() {
            match stream.as_mut().poll_next(cx) {
                Poll::Ready(None) => {
                    this.parts.pop_front();
                    if !this.budget.step(cx) {
                        return Poll::Pending;
                    }
                }
                result => return result,
            }
        }
        Poll::Ready(None)
    }
}
struct Monitored {
    stream: Option<BodyStream>,
    control: Control,
}
pub(super) fn monitored(stream: BodyStream, control: Control) -> BodyStream {
    Box::pin(Monitored {
        stream: Some(stream),
        control,
    })
}
impl Stream for Monitored {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(_polling) = this.control.enter() else {
            let _ = dispose(this.stream.take());
            return Poll::Ready(None);
        };
        if this.control.failed() || this.control.cancelled.load(Ordering::Acquire) {
            this.stream = None;
        }
        let Some(stream) = &mut this.stream else {
            return Poll::Ready(None);
        };
        match stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(Err(source))) => {
                let source = this.control.record(source);
                this.stream = None;
                Poll::Ready(Some(Err(source)))
            }
            Poll::Ready(None) => {
                this.stream = None;
                Poll::Ready(None)
            }
            next => next,
        }
    }
}
pub(super) fn mapped(stream: BodyStream, map: Map) -> BodyStream {
    // Preserve legacy eager mapping elsewhere; repeatable maps run lazily per opening.
    Box::pin(futures_util::stream::once(async move { map(stream) }).flatten())
}
use futures_util::StreamExt;

struct Guarded {
    stream: Option<BodyStream>,
    length: Option<u64>,
    count: u64,
    held: Option<Bytes>,
    control: Control,
    budget: Arc<Budget>,
}
pub(super) fn guarded(
    stream: BodyStream,
    length: Option<u64>,
    control: Control,
    budget: Arc<Budget>,
) -> BodyStream {
    Box::pin(Guarded {
        stream: Some(stream),
        length,
        count: 0,
        held: None,
        control,
        budget,
    })
}
impl Drop for Guarded {
    fn drop(&mut self) {
        self.control.cancel();
        let _ = dispose(self.stream.take());
    }
}
fn length_error() -> BoxError {
    Box::new(io::Error::new(
        io::ErrorKind::InvalidData,
        "the content-length does not match the body length",
    ))
}
impl Guarded {
    fn terminal(&mut self, source: Option<BoxError>) -> Poll<Option<Result<Bytes, BoxError>>> {
        let cleanup = dispose(self.stream.take());
        self.held = None;
        let source = source.or_else(|| cleanup.err().map(|error| self.control.record(error)));
        Poll::Ready(source.map(Err))
    }
    fn stop_observed(&mut self) -> Poll<Option<Result<Bytes, BoxError>>> {
        let source = self.control.error();
        self.terminal(source)
    }
    fn failed(&mut self, source: BoxError) -> Poll<Option<Result<Bytes, BoxError>>> {
        let source = self.control.record(source);
        self.terminal(Some(source))
    }
}

impl Stream for Guarded {
    type Item = Result<Bytes, BoxError>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let Some(_polling) = this.control.enter() else {
            return this.terminal(None);
        };
        this.budget.0.store(POLL_WORK, Ordering::Relaxed);
        loop {
            let Some(stream) = &mut this.stream else {
                return Poll::Ready(None);
            };
            if this.control.failed() {
                return this.stop_observed();
            }
            if !this.budget.step(cx) {
                return Poll::Pending;
            }
            let polled = match guarded_call(|| stream.as_mut().poll_next(cx)) {
                Ok(polled) => polled,
                Err(source) => return this.failed(source),
            };
            if this.control.failed() {
                return this.stop_observed();
            }
            let next = match polled {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(next) => next,
            };
            match next {
                Some(Err(error)) => return this.failed(error),
                None => {
                    if this.length.is_some_and(|length| length != this.count) {
                        return this.failed(length_error());
                    }
                    if let Err(error) = dispose(this.stream.take()) {
                        return this.failed(error);
                    }
                    return Poll::Ready(this.held.take().map(Ok));
                }
                Some(Ok(chunk)) => {
                    let count = u64::try_from(chunk.len())
                        .ok()
                        .and_then(|size| this.count.checked_add(size));
                    let Some(count) =
                        count.filter(|count| this.length.is_none_or(|length| *count <= length))
                    else {
                        return this.failed(length_error());
                    };
                    this.count = count;
                    if this.length == Some(count) {
                        if !chunk.is_empty() {
                            this.held = Some(chunk);
                        }
                        // Verify EOF before releasing the final chunk, including at zero length.
                        continue;
                    }
                    return Poll::Ready(Some(Ok(chunk)));
                }
            }
        }
    }
}

struct Placeholder {
    changed: Arc<AtomicBool>,
    ended: bool,
}
impl Drop for Placeholder {
    fn drop(&mut self) {
        self.changed.store(true, Ordering::Release);
    }
}
impl hyper::body::Body for Placeholder {
    type Data = Bytes;
    type Error = io::Error;
    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, io::Error>>> {
        if self.ended {
            return Poll::Ready(None);
        }
        self.ended = true;
        self.changed.store(true, Ordering::Release);
        Poll::Ready(Some(Err(io::Error::other(
            "a repeatable body cannot be read during edit_request",
        ))))
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        // A private sentinel hint also detects stream wrappers that retain the old body.
        // It is never sent: finishing replaces this unreadable placeholder first.
        hyper::body::SizeHint::with_exact(u64::MAX)
    }
}
pub(crate) fn placeholder() -> (reqwest::Body, Arc<AtomicBool>) {
    let changed = Arc::new(AtomicBool::new(false));
    (
        reqwest::Body::wrap(Placeholder {
            changed: changed.clone(),
            ended: false,
        }),
        changed,
    )
}
pub(crate) fn placeholder_intact(request: &reqwest::Request, changed: &AtomicBool) -> bool {
    !changed.load(Ordering::Acquire)
        && request
            .body()
            .is_some_and(|body| hyper::body::Body::size_hint(body).exact() == Some(u64::MAX))
}
pub(crate) fn declared_length(
    headers: &http::HeaderMap,
) -> Result<Option<u64>, crate::error::BuildFailure> {
    let mut length = None;
    for value in headers.get_all(http::header::CONTENT_LENGTH) {
        let parsed = value
            .to_str()
            .ok()
            .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|text| text.parse::<u64>().ok())
            .ok_or(crate::error::BuildFailure::ContentLength)?;
        if length.is_some_and(|length| length != parsed) {
            return Err(crate::error::BuildFailure::ContentLength);
        }
        length = Some(parsed);
    }
    Ok(length)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(start_paused = true)]
    async fn c12_13_taking_the_failure_does_not_reopen_the_failed_materialization() {
        let control = Control::default();
        let _ = control.record(Box::new(io::Error::other("original")));
        assert!(control.take_failure().is_some());
        assert!(control.failed());
        let opens = Arc::new(AtomicUsize::new(0));
        let seen = opens.clone();
        let body = Body::repeatable(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            async { Ok::<_, io::Error>(futures_util::stream::empty::<Result<Bytes, io::Error>>()) }
        });
        let mut stream = body.materialize(None, control).unwrap();
        assert!(stream.next().await.is_none());
        assert_eq!(opens.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn c12_12_nested_maps_progress_with_more_layers_than_the_private_poll_budget() {
        let layers = Budget::new().0.load(Ordering::Relaxed) * 2 + 1;
        let opens = Arc::new(AtomicUsize::new(0));
        let opened = opens.clone();
        let mut body = Body::repeatable(move || {
            opened.fetch_add(1, Ordering::SeqCst);
            async {
                Ok::<_, io::Error>(futures_util::stream::iter([Ok(Bytes::from_static(
                    b"leaf",
                ))]))
            }
        });
        for _ in 0..layers {
            body = Body::concat([body]).map_stream(|input| input);
        }
        let mut stream = body.into_stream().unwrap();
        let mut context = Context::from_waker(std::task::Waker::noop());
        let mut result = None;
        for _ in 0..(layers + 1) * 4 {
            if let Poll::Ready(item) = stream.as_mut().poll_next(&mut context) {
                result = item;
                break;
            }
        }
        assert_eq!(
            opens.load(Ordering::SeqCst),
            1,
            "descent never reached the leaf"
        );
        assert_eq!(result.unwrap().unwrap(), Bytes::from_static(b"leaf"));
    }
}
