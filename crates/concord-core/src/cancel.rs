//! A streamed request body tied to the call that sends it.
//!
//! reqwest's HTTP/1 connection (hyper) polls a request body from its own task, and notices that
//! the caller dropped the call only between two write passes: while the body has frames ready
//! and the socket accepts them, it goes on polling the body after the caller's future is gone.
//! [`Decoder::execute`](crate::Decoder::execute) therefore wraps every streamed body (one that is
//! not buffered bytes) so that dropping the call ends the body itself: once the drop returns, the
//! body is never polled again.

use std::error::Error as StdError;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body::{Body, Frame, SizeHint};

/// Shared by the sending call and its body.
#[derive(Default)]
struct Shared {
    /// The call was dropped before it completed. Set before `polling` is taken, read under it.
    dropped: AtomicBool,
    /// Held by the body while it polls the wrapped body.
    polling: Mutex<()>,
}

impl Shared {
    fn lock_polling(&self) -> MutexGuard<'_, ()> {
        self.polling.lock().unwrap_or_else(PoisonError::into_inner)
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
            // First the flag, so that every poll that takes the lock from now on returns at once
            // (the connection's task can take it again and again before this thread gets it).
            shared.dropped.store(true, Ordering::SeqCst);
            // Then the lock: it waits for the one poll that read the flag before it was set. No
            // poll reaches the wrapped body after this.
            drop(shared.lock_polling());
        }
    }
}

/// The streamed body as the transport sees it: the wrapped body until the call is dropped, then
/// an error (never the end of the body), so the transport abandons the request.
struct Guarded {
    inner: reqwest::Body,
    shared: Arc<Shared>,
}

impl Body for Guarded {
    type Data = Bytes;
    type Error = Box<dyn StdError + Send + Sync>;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Self::Error>>> {
        let this = self.get_mut();
        // Held across the poll below: a call dropped meanwhile waits for it to return.
        let _polling = this.shared.lock_polling();
        if this.shared.dropped.load(Ordering::SeqCst) {
            return Poll::Ready(Some(Err(Box::new(CallDropped))));
        }
        Pin::new(&mut this.inner)
            .poll_frame(cx)
            .map(|frame| frame.map(|frame| frame.map_err(Into::into)))
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
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
