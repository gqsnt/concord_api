//! Racing work against the caller's stop signal, without an async runtime dependency.
//!
//! The workflows take the stop signal as any `Future<Output = ()>` (a `tokio::time::sleep`
//! deadline, a oneshot receiver mapped to `()`, `std::future::pending()` for "never"). Every
//! await that can wait on the network is raced against it, so cancellation also takes effect
//! while a watch is stalled waiting for data. The work future is polled first: data that is
//! already available wins over a stop that is ready at the same time.

use std::future::{Future, poll_fn};
use std::pin::{Pin, pin};
use std::task::Poll;

/// A fused stop signal: once it has completed it stays stopped and is not polled again.
pub(crate) struct Stop<S> {
    signal: Pin<Box<S>>,
    stopped: bool,
}

impl<S: Future<Output = ()>> Stop<S> {
    pub(crate) fn new(signal: S) -> Self {
        Self {
            signal: Box::pin(signal),
            stopped: false,
        }
    }

    /// `Some(output)` when `work` completes first, `None` when the stop signal does (the work
    /// future is then dropped, with everything it owns).
    pub(crate) async fn race<F: Future>(&mut self, work: F) -> Option<F::Output> {
        if self.stopped {
            return None;
        }
        let mut work = pin!(work);
        poll_fn(|cx| {
            if let Poll::Ready(output) = work.as_mut().poll(cx) {
                return Poll::Ready(Some(output));
            }
            if self.signal.as_mut().poll(cx).is_ready() {
                self.stopped = true;
                return Poll::Ready(None);
            }
            Poll::Pending
        })
        .await
    }
}
