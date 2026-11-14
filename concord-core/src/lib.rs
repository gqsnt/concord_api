//! The core of Concord: the traits and the engine behind the clients that `concord::api!` generates.
//!
//! Applications depend on the crate `concord-api`, which re-exports this crate, the macro and the
//! shipped implementations. This crate knows **no format, no scheme and no pager**: it provides the
//! traits they are built with. What `concord` ships (`json`, `Json`, `by_offset`, …) lives in
//! `concord-extras` and uses public extension traits and helpers. The Tower adapter also uses a
//! hidden helper of the engine for the URL of a response.
//!
//! The engine builds requests, sends them through middleware and a transport, applies
//! retry policies and the call's timeouts, reads responses, and walks pages. Readers, transports,
//! middleware and pagers use the same public traits as the shipped implementations.
//!
//! The surface in one page:
//!
//! | Module | What it holds |
//! |---|---|
//! | [`secret`] | [`Secret`]: text that is sent on the wire and shown nowhere else. |
//! | [`base`] | [`Base`]: the start of a URL, when the caller provides it. |
//! | [`body`] | [`Body`]: the body of a request, as a value. |
//! | [`read`] | [`Reader`]: how a response becomes a value. |
//! | [`answer`] | [`Answer`] and [`Headers`]: a call's value with the response's status and headers. |
//! | [`error`] | [`Error`]: the single error type of a client. |
//! | [`retry`] | [`Retry`]: whether and how a failed request is retried. |
//! | [`page`] | [`Pages`] and [`Items`]: lists that span several responses, and the building blocks of a pager. |
//! | [`middleware`] | [`Middleware`] and [`Next`]: middleware that every request passes through, such as a signer or a token refresher. |
//! | [`transport`] | [`Transport`]: what actually sends a request, and a fake transport for tests. |
//! | [`arg`] | The types that a generated method accepts as an argument. |
//!
//! # Panics
//!
//! Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
//! Polling elsewhere panics with Tokio's own message. Building a client or a call needs no runtime.

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
/// Name them through this re-export, so that the versions always match.
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

/// A call in flight: the future that `send()`, `send_full()` and `send_raw()` return, and that
/// `.await` on a call drives.
///
/// Not cancel safe, like any request: if it is dropped after the request was sent, the
/// server may already have acted on it. After a drop, Concord starts no further attempt, but a
/// request that was already handed to the transport may still be sent.
///
/// # Panics
///
/// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
/// Polling elsewhere panics with Tokio's own message. Building a client or a call needs no runtime.
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
