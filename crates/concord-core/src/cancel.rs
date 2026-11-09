//! A streamed request body tied to the call that sends it.
//!
//! reqwest's HTTP/1 connection (hyper) polls a request body from its own task, and notices that
//! the caller dropped the call only between two write passes: while the body has frames ready
//! and the socket accepts them, it goes on polling the body after the caller's future is gone.
//! [`Decoder::execute`](crate::Decoder::execute) therefore wraps every streamed body (one that is
//! not buffered bytes) so that dropping the call ends the body itself.
//!
//! Cancellation and the checks before and after each inner poll are ordered under one lock.
//! Once cancellation is recorded, no new inner poll is admitted. An admitted poll's result
//! (including final data, trailers or end) is discarded if cancellation precedes its post-poll
//! check: the guard returns an error, reports no end and advertises no length after cancellation.
//! A result accepted by the check before cancellation cannot be recalled, even if delivered
//! later. If the transport has the whole body (its end or every byte of a declared length), the
//! request can still complete.
//!
//! The drop synchronously waits for an admitted poll on another thread, so once it returns the
//! inner body and source are never polled again. A blocking source can hold the drop indefinitely;
//! a source waiting on the dropping thread can deadlock. A drop from inside the poll itself
//! records cancellation without waiting: that poll can keep running, but its result is discarded.

use std::error::Error as StdError;
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};
use std::thread::{self, ThreadId};

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};

/// Shared by the sending call and its body. The lock is held only to read or change [`State`],
/// never across a poll of the wrapped body.
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    /// Signalled when a poll ends while a dropped call waits for it.
    poll_ended: Condvar,
}

#[derive(Default)]
struct State {
    /// The call was dropped before it completed.
    dropped: bool,
    /// The thread polling the wrapped body right now, if any.
    polling: Option<ThreadId>,
    /// A dropped call is waiting for that poll to end.
    waiting: bool,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Starts a poll of the wrapped body, unless the call was dropped.
    fn begin_poll(&self) -> bool {
        let mut state = self.lock();
        if state.dropped {
            return false;
        }
        state.polling = Some(thread::current().id());
        true
    }

    /// Ends the poll in progress; `true` if the call was dropped meanwhile (its result must not
    /// reach the transport).
    fn end_poll(&self) -> bool {
        let mut state = self.lock();
        state.polling = None;
        if state.waiting {
            self.poll_ended.notify_all();
        }
        state.dropped
    }

    fn is_dropped(&self) -> bool {
        self.lock().dropped
    }

    /// The call was dropped: no poll starts from now on, and this waits for the one in progress.
    fn cancel(&self) {
        let mut state = self.lock();
        state.dropped = true;
        // A poll on this very thread is not one to wait for: the call is being dropped from
        // inside it (the body's own code dropped it), so waiting would never end. Its result is
        // discarded when it returns.
        let here = thread::current().id();
        while state.polling.is_some_and(|thread| thread != here) {
            state.waiting = true;
            state = self
                .poll_ended
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        state.waiting = false;
    }
}

/// Held by a sending call while it waits for the response; dropped before
/// [`finish`](Self::finish), it cancels the streamed body it guards.
pub(crate) struct SendGuard(Option<Arc<Shared>>);

impl SendGuard {
    /// Wraps the body of `request` if it is streamed. A buffered body (or none) is left as it is:
    /// it keeps its exact length and the transport's ability to resend it.
    pub(crate) fn attach(request: &mut reqwest::Request) -> Self {
        let streamed = request.body().is_some_and(|body| body.as_bytes().is_none());
        match request.body_mut().take() {
            Some(inner) if streamed => {
                let shared = Arc::new(Shared::default());
                *request.body_mut() = Some(reqwest::Body::wrap(Guarded {
                    inner,
                    shared: Arc::clone(&shared),
                }));
                Self(Some(shared))
            }
            body => {
                *request.body_mut() = body;
                Self(None)
            }
        }
    }

    /// The call completed (a response or a transport error): whatever remains of the body is the
    /// transport's.
    pub(crate) fn finish(mut self) {
        self.0 = None;
    }
}

impl Drop for SendGuard {
    fn drop(&mut self) {
        if let Some(shared) = self.0.take() {
            shared.cancel();
        }
    }
}

/// The streamed body as the transport sees it: the wrapped body until the call is dropped, then
/// an error (never the end of the body), so the transport abandons the request.
struct Guarded {
    inner: reqwest::Body,
    shared: Arc<Shared>,
}

/// Ends the poll in progress if the wrapped poll unwinds, so that a dropped call does not wait
/// for it forever.
struct Polling<'a>(&'a Shared);

impl Polling<'_> {
    /// Ends the poll; `true` if the call was dropped meanwhile.
    fn end(self) -> bool {
        let dropped = self.0.end_poll();
        std::mem::forget(self);
        dropped
    }
}

impl Drop for Polling<'_> {
    fn drop(&mut self) {
        self.0.end_poll();
    }
}

impl Body for Guarded {
    type Data = Bytes;
    type Error = Box<dyn StdError + Send + Sync>;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.get_mut();
        if !this.shared.begin_poll() {
            return Poll::Ready(Some(Err(Box::new(CallDropped))));
        }
        let polling = Polling(&this.shared);
        let polled = Pin::new(&mut this.inner).poll_frame(cx);
        if polling.end() {
            // The call was dropped while this poll ran: its data, trailers or end are not
            // handed over.
            return Poll::Ready(Some(Err(Box::new(CallDropped))));
        }
        polled.map(|frame| frame.map(|frame| frame.map_err(Into::into)))
    }

    fn is_end_stream(&self) -> bool {
        !self.shared.is_dropped() && self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        if self.shared.is_dropped() {
            return SizeHint::default();
        }
        self.inner.size_hint()
    }
}

/// The body error after the sending call was dropped.
#[derive(Debug)]
struct CallDropped;

impl fmt::Display for CallDropped {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the call sending this request was dropped before its response arrived")
    }
}

impl StdError for CallDropped {}

#[cfg(test)]
mod tests {
    //! The guard on its own, driven step by step: a scripted body whose polls can block until the
    //! test releases them, polled by a thread of the test as a transport would poll it. Every
    //! interleaving below is forced by channels and by the guard's own state, never by timing; the
    //! timeouts only bound hangs; no successful assertion depends on elapsed time.

    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc;
    use std::task::Waker;
    use std::time::{Duration, Instant};

    const HANG_GUARD: Duration = Duration::from_secs(10);

    /// What one poll of the scripted body does.
    enum Step {
        Data(&'static [u8]),
        /// The last data frame: the body reports its end once it has returned it.
        LastData(&'static [u8]),
        Trailers,
        End,
        Panic,
        /// Tells the test the poll has started, waits until the test releases it, then goes on.
        Gate(Box<Step>),
        /// Drops the sending call from inside the poll (a body whose own code drops it).
        DropCall(Box<Step>),
    }

    /// What the scripted body reports to the test.
    #[derive(Default)]
    struct Probe {
        polls: AtomicU64,
        in_poll: AtomicU64,
        destroyed: AtomicBool,
    }

    struct Script {
        steps: VecDeque<Step>,
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        call: Arc<Mutex<Option<SendGuard>>>,
    }

    struct Scripted {
        script: Mutex<Script>,
        exact: Option<u64>,
        ended: AtomicBool,
        probe: Arc<Probe>,
    }

    impl Body for Scripted {
        type Data = Bytes;
        type Error = std::io::Error;

        fn poll_frame(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Bytes>, std::io::Error>>> {
            let this = self.get_mut();
            this.probe.polls.fetch_add(1, Ordering::SeqCst);
            this.probe.in_poll.fetch_add(1, Ordering::SeqCst);
            let script = this.script.get_mut().unwrap();
            // Past the script: a poll that should not have happened, counted and answered.
            let mut step = script.steps.pop_front().unwrap_or(Step::End);
            loop {
                match step {
                    Step::Gate(next) => {
                        script.entered.send(()).unwrap();
                        script.release.recv().unwrap();
                        step = *next;
                    }
                    Step::DropCall(next) => {
                        let call = script.call.lock().unwrap().take();
                        drop(call);
                        step = *next;
                    }
                    _ => break,
                }
            }
            let frame = match step {
                Step::Data(bytes) => Some(Ok(Frame::data(Bytes::from_static(bytes)))),
                Step::LastData(bytes) => {
                    this.ended.store(true, Ordering::SeqCst);
                    Some(Ok(Frame::data(Bytes::from_static(bytes))))
                }
                Step::Trailers => {
                    this.ended.store(true, Ordering::SeqCst);
                    Some(Ok(Frame::trailers(reqwest::header::HeaderMap::new())))
                }
                Step::End => {
                    this.ended.store(true, Ordering::SeqCst);
                    None
                }
                Step::Panic => panic!("the scripted body panicked inside its poll"),
                Step::Gate(_) | Step::DropCall(_) => unreachable!(),
            };
            if let Some(Ok(frame)) = &frame
                && let Some(bytes) = frame.data_ref()
                && let Some(remaining) = &mut this.exact
            {
                *remaining -= bytes.len() as u64;
            }
            this.probe.in_poll.fetch_sub(1, Ordering::SeqCst);
            Poll::Ready(frame)
        }

        fn is_end_stream(&self) -> bool {
            self.ended.load(Ordering::SeqCst)
        }

        fn size_hint(&self) -> SizeHint {
            self.exact.map(SizeHint::with_exact).unwrap_or_default()
        }
    }

    impl Drop for Scripted {
        fn drop(&mut self) {
            self.probe.destroyed.store(true, Ordering::SeqCst);
        }
    }

    /// One poll of the body as the transport sees it, with what the body reports right after.
    #[derive(Debug, PartialEq)]
    enum Polled {
        Data(Vec<u8>),
        Trailers,
        End,
        /// An error; `true` when it is the dropped-call error.
        Failed(bool),
    }

    #[derive(Debug, PartialEq)]
    struct Seen {
        polled: Polled,
        end_stream: bool,
        exact: Option<u64>,
    }

    fn poll(body: &mut reqwest::Body) -> Seen {
        let mut cx = Context::from_waker(Waker::noop());
        let polled = match Pin::new(&mut *body).poll_frame(&mut cx) {
            Poll::Pending => unreachable!("the scripted body is never pending"),
            Poll::Ready(None) => Polled::End,
            Poll::Ready(Some(Ok(frame))) => match frame.into_data() {
                Ok(data) => Polled::Data(data.to_vec()),
                Err(_) => Polled::Trailers,
            },
            Poll::Ready(Some(Err(error))) => {
                let mut cause: Option<&(dyn StdError + 'static)> = Some(&error);
                let mut dropped = false;
                while let Some(e) = cause {
                    dropped |= e.is::<CallDropped>();
                    cause = e.source();
                }
                Polled::Failed(dropped)
            }
        };
        Seen {
            polled,
            end_stream: body.is_end_stream(),
            exact: body.size_hint().exact(),
        }
    }

    fn saw(polled: Polled, end_stream: bool, exact: Option<u64>) -> Seen {
        Seen {
            polled,
            end_stream,
            exact,
        }
    }

    fn data(bytes: &[u8]) -> Polled {
        Polled::Data(bytes.to_vec())
    }

    /// A guarded scripted body, as `Decoder::execute` would send it.
    struct Harness {
        guard: SendGuard,
        body: reqwest::Body,
        probe: Arc<Probe>,
        entered: mpsc::Receiver<()>,
        release: mpsc::Sender<()>,
        call: Arc<Mutex<Option<SendGuard>>>,
    }

    fn harness(steps: Vec<Step>, exact: Option<u64>) -> Harness {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let call = Arc::new(Mutex::new(None));
        let probe = Arc::new(Probe::default());
        let scripted = Scripted {
            script: Mutex::new(Script {
                steps: steps.into(),
                entered: entered_tx,
                release: release_rx,
                call: Arc::clone(&call),
            }),
            exact,
            ended: AtomicBool::new(false),
            probe: Arc::clone(&probe),
        };
        let mut request = reqwest::Request::new(
            reqwest::Method::POST,
            "http://cancel.test/".parse().unwrap(),
        );
        *request.body_mut() = Some(reqwest::Body::wrap(scripted));
        let guard = SendGuard::attach(&mut request);
        let body = request.body_mut().take().unwrap();
        Harness {
            guard,
            body,
            probe,
            entered,
            release,
            call,
        }
    }

    /// Polls like a transport: until the first poll that is not data. The body is dropped at
    /// the end (the transport abandons it).
    fn transport(mut body: reqwest::Body) -> thread::JoinHandle<Vec<Seen>> {
        thread::spawn(move || {
            let mut seen = Vec::new();
            loop {
                let next = poll(&mut body);
                let more = matches!(next.polled, Polled::Data(_));
                seen.push(next);
                if !more {
                    return seen;
                }
            }
        })
    }

    /// What the dropping thread observed when the drop returned.
    #[derive(Debug, PartialEq)]
    struct AtReturn {
        in_poll: u64,
        polls: u64,
    }

    /// Wait until the drop has committed to waiting, or has returned prematurely. Observing
    /// `waiting` under the state lock also observes the cancellation flag before the test lets
    /// the inner poll return. This catches waiting before marking cancellation without load.
    fn cancellation_before_release(shared: &Shared, returned: &mpsc::Receiver<AtReturn>) {
        let deadline = Instant::now() + HANG_GUARD;
        loop {
            let state = shared.lock();
            if state.waiting {
                assert!(
                    state.dropped,
                    "the drop waited before recording cancellation"
                );
                return;
            }
            drop(state);
            if let Ok(at) = returned.try_recv() {
                panic!("the drop returned while a poll of the body was still in progress: {at:?}");
            }
            assert!(
                Instant::now() < deadline,
                "the drop neither waited nor returned"
            );
            thread::yield_now();
        }
    }

    /// Drops the call on its own thread while the gated poll is in progress, and returns what
    /// the transport saw and what held when the drop returned. Panics if the drop returns while
    /// that poll is still in progress (it must wait for it).
    fn drop_during_gated_poll(h: Harness) -> (Vec<Seen>, AtReturn, Arc<Probe>) {
        let Harness {
            guard,
            body,
            probe,
            entered,
            release,
            ..
        } = h;
        let shared = Arc::clone(guard.0.as_ref().expect("a streamed body is guarded"));
        let transport = transport(body);
        entered.recv_timeout(HANG_GUARD).unwrap();
        let (returned_tx, returned) = mpsc::channel();
        let p = Arc::clone(&probe);
        let dropper = thread::spawn(move || {
            drop(guard);
            returned_tx
                .send(AtReturn {
                    in_poll: p.in_poll.load(Ordering::SeqCst),
                    polls: p.polls.load(Ordering::SeqCst),
                })
                .unwrap();
        });
        cancellation_before_release(&shared, &returned);
        release.send(()).unwrap();
        let seen = transport.join().unwrap();
        let at_return = returned.recv_timeout(HANG_GUARD).unwrap();
        dropper.join().unwrap();
        assert!(probe.destroyed.load(Ordering::SeqCst));
        (seen, at_return, probe)
    }

    #[test]
    fn a_buffered_body_is_not_wrapped() {
        let mut request = reqwest::Request::new(
            reqwest::Method::POST,
            "http://cancel.test/".parse().unwrap(),
        );
        *request.body_mut() = Some(reqwest::Body::from("bytes"));
        let guard = SendGuard::attach(&mut request);
        assert!(guard.0.is_none());
        assert_eq!(
            request.body().and_then(reqwest::Body::as_bytes),
            Some(&b"bytes"[..])
        );
    }

    #[test]
    fn the_drop_waits_for_the_poll_in_progress_and_hands_over_nothing_from_it() {
        let h = harness(
            vec![Step::Data(b"a"), Step::Gate(Box::new(Step::Data(b"b")))],
            None,
        );
        let (seen, at_return, probe) = drop_during_gated_poll(h);
        assert_eq!(
            seen,
            vec![
                saw(data(b"a"), false, None),
                saw(Polled::Failed(true), false, None),
            ]
        );
        assert_eq!(
            at_return,
            AtReturn {
                in_poll: 0,
                polls: 2
            }
        );
        assert_eq!(probe.polls.load(Ordering::SeqCst), 2);
        assert!(probe.destroyed.load(Ordering::SeqCst));
    }

    #[test]
    fn no_poll_reaches_the_body_after_the_drop() {
        let mut h = harness(vec![Step::Data(b"a"), Step::Data(b"b")], None);
        assert_eq!(poll(&mut h.body), saw(data(b"a"), false, None));
        drop(h.guard);
        for _ in 0..3 {
            assert_eq!(poll(&mut h.body), saw(Polled::Failed(true), false, None));
        }
        assert_eq!(h.probe.polls.load(Ordering::SeqCst), 1);
        drop(h.body);
        assert!(h.probe.destroyed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_poll_in_progress_cannot_end_a_dropped_body() {
        // Its end, its last data frame (after which the body reports its end), its trailers.
        for last in [Step::End, Step::LastData(b"z"), Step::Trailers] {
            let h = harness(vec![Step::Data(b"a"), Step::Gate(Box::new(last))], None);
            let (seen, at_return, _) = drop_during_gated_poll(h);
            assert_eq!(
                seen,
                vec![
                    saw(data(b"a"), false, None),
                    saw(Polled::Failed(true), false, None),
                ]
            );
            assert_eq!(
                at_return,
                AtReturn {
                    in_poll: 0,
                    polls: 2
                }
            );
        }
    }

    #[test]
    fn a_dropped_body_of_known_length_neither_completes_nor_claims_its_length() {
        let h = harness(
            vec![Step::Data(b"a"), Step::Gate(Box::new(Step::LastData(b"b")))],
            Some(2),
        );
        let (seen, _, _) = drop_during_gated_poll(h);
        // Before the drop, the declared length; after it, no length and no end: the second of the
        // two declared bytes is never handed over.
        assert_eq!(
            seen,
            vec![
                saw(data(b"a"), false, Some(1)),
                saw(Polled::Failed(true), false, None),
            ]
        );
    }

    #[test]
    fn a_dropped_body_never_reports_its_end() {
        let mut h = harness(vec![Step::LastData(b"z")], None);
        assert_eq!(poll(&mut h.body), saw(data(b"z"), true, None));
        drop(h.guard);
        assert!(!h.body.is_end_stream());
        assert_eq!(poll(&mut h.body), saw(Polled::Failed(true), false, None));
        assert_eq!(h.probe.polls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cancellation_cannot_recall_an_already_accepted_known_length_body() {
        let mut h = harness(vec![Step::Data(b"a"), Step::LastData(b"b")], Some(2));
        let declared = h.body.size_hint().exact().unwrap();
        let first = poll(&mut h.body);
        let last = poll(&mut h.body);
        assert_eq!(first, saw(data(b"a"), false, Some(1)));
        assert_eq!(last, saw(data(b"b"), true, Some(0)));
        drop(h.guard);
        // A transport holding these results already has all declared bytes. Cancellation does
        // not retract them and cannot force it to poll again to discover the error.
        assert_eq!(declared, 2);
        assert_eq!(first.polled, data(b"a"));
        assert_eq!(last.polled, data(b"b"));
        assert!(!h.body.is_end_stream());
        assert_eq!(h.body.size_hint().exact(), None);
        assert_eq!(poll(&mut h.body), saw(Polled::Failed(true), false, None));
        assert_eq!(h.probe.polls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_body_that_drops_the_call_from_its_own_poll_does_not_deadlock() {
        let h = harness(
            vec![Step::Data(b"a"), Step::DropCall(Box::new(Step::Data(b"b")))],
            None,
        );
        *h.call.lock().unwrap() = Some(h.guard);
        let probe = Arc::clone(&h.probe);
        let transport = transport(h.body);
        let (done_tx, done) = mpsc::channel();
        thread::spawn(move || done_tx.send(transport.join()).unwrap());
        let seen = done
            .recv_timeout(HANG_GUARD)
            .expect("the drop from inside the poll deadlocked")
            .unwrap();
        assert_eq!(
            seen,
            vec![
                saw(data(b"a"), false, None),
                saw(Polled::Failed(true), false, None),
            ]
        );
        assert_eq!(probe.polls.load(Ordering::SeqCst), 2);
        assert!(probe.destroyed.load(Ordering::SeqCst));
    }

    #[test]
    fn a_poll_that_unwinds_does_not_leave_the_drop_waiting() {
        let h = harness(vec![Step::Gate(Box::new(Step::Panic))], None);
        let p = Arc::clone(&h.probe);
        let Harness {
            guard,
            body,
            entered,
            release,
            ..
        } = h;
        let shared = Arc::clone(guard.0.as_ref().unwrap());
        let transport = transport(body);
        entered.recv_timeout(HANG_GUARD).unwrap();
        let (returned_tx, returned) = mpsc::channel();
        thread::spawn(move || {
            drop(guard);
            returned_tx
                .send(AtReturn {
                    in_poll: p.in_poll.load(Ordering::SeqCst),
                    polls: p.polls.load(Ordering::SeqCst),
                })
                .unwrap();
        });
        cancellation_before_release(&shared, &returned);
        release.send(()).unwrap();
        assert!(transport.join().is_err(), "the scripted poll panicked");
        returned
            .recv_timeout(HANG_GUARD)
            .expect("the drop still waits for a poll that unwound");
    }
}
