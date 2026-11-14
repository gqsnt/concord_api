//! The core of Concord: the traits and the engine of the clients that `concord::api!` makes.
//!
//! People depend on the crate `concord-api`, which re-exports this one, the macro and the shipped
//! pieces. This crate knows **no format, no scheme and no pager**: it has the traits that they
//! are written with. What `concord` ships (`json`, `Json`, `by_offset`, …) is in `concord-extras`,
//! and uses public extension traits and helpers. The Tower adapter also uses a
//! hidden helper of the engine for the URL of an answer.
//!
//! The engine builds requests, sends them through middleware and a transport, applies
//! retry policies and call clocks, reads answers, and walks pages. Readers, transports,
//! middleware and pagers use the same public traits as the shipped implementations.
//!
//! The surface in one page:
//!
//! | Module | What it holds |
//! |---|---|
//! | [`secret`] | [`Secret`]: text that goes on the wire and nowhere else. |
//! | [`base`] | [`Base`]: where a URL starts, when the user gives it. |
//! | [`body`] | [`Body`]: the body of a request, as a value. |
//! | [`read`] | [`Reader`]: how an answer becomes a value. |
//! | [`answer`] | [`Answer`] and [`Headers`]: what came back. |
//! | [`error`] | [`Error`]: the one error of a client. |
//! | [`retry`] | [`Retry`]: whether and how to try again. |
//! | [`page`] | [`Pages`] and [`Items`]: a list that comes in several answers; what a pager is made with. |
//! | [`middleware`] | [`Middleware`] and [`Next`]: what every request goes through, such as a signer or a token that is renewed. |
//! | [`transport`] | [`Transport`]: what really sends a request. A fake for tests. |
//! | [`arg`] | What an argument of a generated method may be. |
//!
//! # Panics
//!
//! Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
//! Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

/// What generated code is made of. **Not public API:** it can change in any release.
/// The doors of Concord are the public traits and methods, not this module.
#[doc(hidden)]
pub mod __private;
pub mod answer;
pub mod arg;
pub mod base;
pub mod body;
pub mod error;
pub mod middleware;
pub mod page;
pub mod read;
pub mod retry;
pub mod secret;
pub mod transport;

#[cfg(test)]
mod engine_contracts;

pub use answer::{Answer, Headers};
pub use base::Base;
pub use body::Body;
pub use error::{mismatch, Error, Fail};
pub use middleware::{Middleware, Next};
pub use page::{Items, Pages};
pub use read::{ByteStream, Reader};
pub use retry::Retry;
pub use secret::Secret;
pub use transport::{fake, reply, reply_with, Http, Transport};

/// The HTTP library below Concord, at the version Concord is built with.
///
/// `reqwest::Request` and `reqwest::Response` are in the signature of [`Transport`].
/// Name them through here and the versions always agree.
#[doc(no_inline)]
pub use reqwest;

/// The `http` crate, at the version Concord is built with: `StatusCode`, `Method`, `HeaderMap`.
#[doc(no_inline)]
pub use http;

/// The `bytes` crate, at the version Concord is built with.
#[doc(no_inline)]
pub use bytes;

/// 1024 bytes.
pub const KIB: usize = 1024;
/// 1024 × 1024 bytes.
// PANIC-SAFETY: These constant byte limits fit usize on supported 32/64-bit targets.
pub const MIB: usize = 1024 * 1024;

/// A call that is on its way: what `send()`, `send_full()`, `send_raw()` and `.await` on a call give.
///
/// Not cancel safe, like any request: if it is dropped after the request went out, the
/// server may have acted. After a drop Concord starts no further try; a request that was
/// already handed to the transport may still go out.
///
/// # Panics
///
/// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
/// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Sending<T, E = ()> {
    future: std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, Error<E>>> + Send>>,
}

impl<T, E> Sending<T, E> {
    /// For generated code. Not public API.
    #[doc(hidden)]
    pub fn new(
        future: impl std::future::Future<Output = Result<T, Error<E>>> + Send + 'static,
    ) -> Sending<T, E> {
        Sending {
            future: Box::pin(future),
        }
    }
}

impl<T, E> std::future::Future for Sending<T, E> {
    type Output = Result<T, Error<E>>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        self.future.as_mut().poll(cx)
    }
}

impl<T, E> std::fmt::Debug for Sending<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sending").finish_non_exhaustive()
    }
}

// What the engineering sheet promises about threads, held by the compiler.
const _: fn() = || {
    fn send_sync<T: Send + Sync>() {}
    fn send<T: Send>() {}
    fn unpin<T: Unpin>() {}
    send_sync::<Secret>();
    send_sync::<Body>();
    send_sync::<Error<String>>();
    send_sync::<Retry<String>>();
    send_sync::<Pages<String, String>>();
    send_sync::<Items<String, String>>();
    send::<Sending<String, String>>();
    send::<ByteStream>();
    unpin::<Pages<String, String>>();
    unpin::<Items<String, String>>();
    unpin::<ByteStream>();
};
