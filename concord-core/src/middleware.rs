//! Middleware: what every request passes through before the transport actually sends it.

use crate::transport::{Transport, TransportError, TryInfo};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A layer that every request passes through, on each attempt and each page, before the
/// transport actually sends it.
///
/// Write `middleware Signed::new(key.clone());` on the client to create one in `new`.
/// Its expression sees the required client inputs as references. Handles and calls share
/// that middleware. Middleware added with a caller's `with_middleware` runs first, in the
/// order added; then the description's middleware runs, in the order written. Put a signer
/// last. `with_transport` replaces only what actually sends the request, underneath them.
/// The description's middleware stays on every call. To treat public endpoints differently,
/// inspect the request or [`TryInfo::endpoint`] inside `handle`.
///
/// A signer is ordinary Rust next to the description. The request and the response are
/// `concord::reqwest::Request` and `concord::reqwest::Response`: a crate that depends only on
/// `concord` needs no `reqwest` dependency of its own.
///
/// ```
/// # use concord_core as concord;
/// use concord::{Middleware, Next};
/// use concord::transport::{TransportError, TryInfo};
///
/// struct Signed { key: concord::Secret }
///
/// impl Middleware for Signed {
///     async fn handle(&self, mut request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
///         -> Result<concord::reqwest::Response, TransportError>
///     {
///         let text = format!("{} {}", request.method(), request.url().path());
///         let signature = sign(&self.key, &text)
///             .map_err(|e| TransportError::blocked().with_source(e))?;
///         request.headers_mut().insert("x-signature", signature);
///         next.send(request, info).await
///     }
/// }
/// # fn sign(_: &concord::Secret, _: &str) -> Result<concord::http::HeaderValue, std::fmt::Error> { Ok(concord::http::HeaderValue::from_static("signature")) }
/// # use futures_util::FutureExt;
/// # async {
/// let transport = concord::fake(|request| {
///     assert_eq!(request.headers().get("x-signature").unwrap(), "signature");
///     Ok(concord::reply(200, "signed"))
/// });
/// let request = concord::reqwest::Request::new(concord::reqwest::Method::GET, "https://example.test/".parse()?);
/// let signer = Signed { key: concord::Secret::new("key") };
/// let response = signer.handle(request, TryInfo::new("signed.get", 1), Next::to(&transport)).await?;
/// assert_eq!(response.bytes().await?, "signed");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # }.now_or_never().unwrap().unwrap();
/// ```
///
/// A token can be renewed after a `401`. Take a copy before the first send; a stream body
/// cannot be copied, so in that case return the first response. Put this middleware before
/// a signer so that the second send is signed too:
///
/// ```
/// # use concord_core as concord;
/// # use concord::{Middleware, Next};
/// # use concord::transport::{TransportError, TryInfo};
/// struct Token { /* shared token and renewal state */ }
/// impl Middleware for Token {
///     async fn handle(&self, mut request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
///         -> Result<concord::reqwest::Response, TransportError>
///     {
///         request.headers_mut().insert("authorization", self.current());
///         let again = request.try_clone();
///         let response = next.send(request, info.clone()).await?;
///         if response.status() == 401 {
///             if let Some(mut request) = again {
///                 request.headers_mut().insert("authorization", self.renew().await?);
///                 return next.send(request, info).await;
///             }
///         }
///         Ok(response)
///     }
/// }
/// # impl Token {
/// #     fn current(&self) -> concord::http::HeaderValue { concord::http::HeaderValue::from_static("token") }
/// #     async fn renew(&self) -> Result<concord::http::HeaderValue, TransportError> { Ok(self.current()) }
/// # }
/// # use futures_util::FutureExt;
/// # async {
/// let sends = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
/// let count = sends.clone();
/// let transport = concord::fake(move |request| {
///     assert_eq!(request.headers().get("authorization").unwrap(), "token");
///     let status = if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 { 401 } else { 200 };
///     Ok(concord::reply(status, ""))
/// });
/// let request = concord::reqwest::Request::new(concord::reqwest::Method::GET, "https://example.test/".parse()?);
/// let response = Token {}.handle(request, TryInfo::new("token.get", 1), Next::to(&transport)).await?;
/// assert_eq!(response.status(), 200);
/// assert_eq!(sends.load(std::sync::atomic::Ordering::SeqCst), 2);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # }.now_or_never().unwrap().unwrap();
/// ```
///
/// `next.send(request, info)` may be called more than once, or not at all. A middleware layer
/// that rejects the request before sending returns [`TransportError::blocked`]; the attempts
/// end there. After a response has arrived, keep that response, or report another failure if
/// renewal fails. Concord counts the whole chain as one attempt; sends within it do not add
/// attempts. The call `timeout` includes the time spent in middleware. The idle timeout starts
/// only when the request is handed to the transport, after middleware; `timeout` is checked
/// there too, so an expired call is never handed over.
/// `request()` and `into_request()` return the request before any middleware runs.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a middleware",
    note = "implement concord::Middleware with handle(request, info, next); next.send(request, info) sends on"
)]
pub trait Middleware: Send + Sync + 'static {
    /// Handles this request, sending it through the rest of the chain with `next.send`.
    fn handle(
        &self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'_>,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send;
}

type ResponseFuture<'a> =
    Pin<Box<dyn Future<Output = Result<reqwest::Response, TransportError>> + Send + 'a>>;

pub(crate) trait ErasedTransport: Send + Sync {
    fn send(&self, request: reqwest::Request, info: TryInfo) -> ResponseFuture<'_>;
}

impl<T: Transport> ErasedTransport for T {
    fn send(&self, request: reqwest::Request, info: TryInfo) -> ResponseFuture<'_> {
        let url = request.url().clone();
        Box::pin(async move {
            Transport::send(self, request, info)
                .await
                .map(|response| crate::transport::__response_at(response, url))
        })
    }
}

pub(crate) trait ErasedMiddleware: Send + Sync {
    fn handle<'a>(
        &'a self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'a>,
    ) -> ResponseFuture<'a>;
}

impl<M: Middleware> ErasedMiddleware for M {
    fn handle<'a>(
        &'a self,
        request: reqwest::Request,
        info: TryInfo,
        next: Next<'a>,
    ) -> ResponseFuture<'a> {
        Box::pin(Middleware::handle(self, request, info, next))
    }
}

/// The remaining middleware, in order, followed by the transport that actually sends the
/// request.
///
/// It is a shared view: sending does not consume the chain. A middleware layer can send again
/// through the same remainder, for example after renewing a token. The idle timeout starts
/// when the request is handed to the transport, where `timeout` is checked before sending.
#[derive(Clone, Copy)]
pub struct Next<'a> {
    pub(crate) middlewares: &'a [Arc<dyn ErasedMiddleware>],
    pub(crate) transport: &'a dyn ErasedTransport,
}

impl std::fmt::Debug for Next<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Next").finish_non_exhaustive()
    }
}

impl<'a> Next<'a> {
    /// Creates a chain that sends directly to this transport, for example to test one
    /// middleware layer over a fake.
    pub fn to(transport: &'a impl Transport) -> Next<'a> {
        Next {
            middlewares: &[],
            transport,
        }
    }

    async fn send_transport(
        self,
        mut request: reqwest::Request,
        info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        use futures_util::StreamExt;
        use std::task::Poll;
        use tokio::time::Instant;
        let Some(run) = info.run().cloned() else {
            return self.transport.send(request, info).await;
        };
        if run.expired() {
            return Err(TransportError::clock(run.total_timeout()));
        }
        let started = Instant::now();
        let progress = Arc::new(std::sync::Mutex::new(started));
        if request.body().is_some_and(|body| body.as_bytes().is_none()) {
            if let Some(body) = request.body_mut().take() {
                // A Response exposes reqwest's body as a byte stream without an
                // extra body crate. Bytes held in memory deliberately stay bytes.
                let sent = progress.clone();
                let stream = reqwest::Response::from(http::Response::new(body))
                    .bytes_stream()
                    .inspect(move |piece| {
                        if piece.is_ok() {
                            *sent
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Instant::now();
                        }
                    });
                *request.body_mut() = Some(reqwest::Body::wrap_stream(stream));
            }
        }
        let work = std::pin::pin!(self.transport.send(request, info));
        let mut work = work;
        let mut timer = started
            .checked_add(run.idle())
            .map(|end| Box::pin(tokio::time::sleep_until(end)));
        std::future::poll_fn(|cx| {
            if let Poll::Ready(result) = work.as_mut().poll(cx) {
                return Poll::Ready(result);
            }
            let last = *progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let (Some(timer), Some(end)) = (&mut timer, last.checked_add(run.idle())) {
                timer.as_mut().reset(end);
                if timer.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(TransportError::clock(run.idle_timeout())));
                }
            }
            Poll::Pending
        })
        .await
        .map_err(|failure| run.outer_clock(failure))
    }

    /// Sends through the remaining middleware and then the transport.
    pub fn send(
        self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send + 'a {
        match self.middlewares.split_first() {
            Some((middleware, rest)) => middleware.handle(
                request,
                info,
                Next {
                    middlewares: rest,
                    ..self
                },
            ),
            None => Box::pin(self.send_transport(request, info)),
        }
    }
}
