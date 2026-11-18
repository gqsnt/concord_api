//! Middleware: what every request passes through before the transport actually sends it.

use crate::transport::{Sends, Transport, TransportError, TryInfo};
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
/// A middleware layer can also read the response that `next.send` returns. It returns that
/// response, or a [`TransportError`], and does not send the request a second time to replace
/// it: a second `next.send` is not an attempt, the retry policy does not decide it, and it
/// repeats a `POST` that the engine would not repeat. For a token that is renewed, Concord
/// ships `concord::auth::Renewed`, and the retry rule `concord::auth::after_401` asks the
/// engine for one more attempt after a `401`. For a limit shared by the calls of a client, it
/// ships `concord::limit::Limit`.
///
/// ```
/// # use concord_core as concord;
/// # use concord::{Middleware, Next};
/// # use concord::transport::{TransportError, TryInfo};
/// /// Logs the endpoint, the attempt and the status of each response.
/// struct Logged;
///
/// impl Middleware for Logged {
///     async fn handle(&self, request: concord::reqwest::Request, info: TryInfo, next: Next<'_>)
///         -> Result<concord::reqwest::Response, TransportError>
///     {
///         let (endpoint, number) = (info.endpoint(), info.number());
///         let response = next.send(request, info).await?;   // the rest of the chain, then the transport
///         eprintln!("{endpoint} (try {number}): {}", response.status());   // names and numbers only
///         Ok(response)                                       // the response as it arrived
///     }
/// }
/// # use futures_util::FutureExt;
/// # async {
/// let transport = concord::fake(|_| Ok(concord::reply(503, "")));
/// let request = concord::reqwest::Request::new(concord::reqwest::Method::GET, "https://example.test/".parse()?);
/// let response = Logged.handle(request, TryInfo::new("logged.get", 1), Next::to(&transport)).await?;
/// assert_eq!(response.status(), 503);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # }.now_or_never().unwrap().unwrap();
/// ```
///
/// `next.send(request, info)` may be called more than once, or not at all. A middleware layer
/// that rejects the request before sending returns [`TransportError::blocked`]; the attempts
/// end there. After a response has arrived, return that response, or another failure. Concord
/// counts the whole chain as one attempt; sends within it do not add attempts. The call `timeout` includes the time spent in middleware. The idle timeout starts
/// at the final middleware handoff, including validation of a repeatable body declared
/// empty. Both deadlines are checked before the validated request reaches the transport.
/// `request()` and `into_request()` return the request before any middleware runs.
///
/// Before a middleware layer returns [`TransportError::blocked`] or
/// [`TransportError::not_sent`], or forwards such a failure from `next.send`, it accounts
/// for every send that it made or still holds, through `next`, through `Next::to`, or with a
/// transport of its own: Concord sees the first two, and the third is its author's to
/// account for. A false statement can make a credential send a refresh token twice (see
/// [`TryInfo`]).
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a middleware",
    note = "implement `concord::Middleware` with `handle(request, info, next)`; `next.send(request, info)` passes the request on"
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
/// It is a shared view: sending does not consume the chain, so a middleware layer can send
/// again through the same remainder. Such a send is not an attempt, and the retry policy does
/// not decide it: for another attempt after a `401`, with a new token, use the retry rule
/// `concord::auth::after_401`. The idle timeout starts at the final middleware handoff,
/// including validation of a repeatable body declared empty. Both deadlines are checked
/// before that validated request reaches the transport.
#[derive(Clone, Copy)]
pub struct Next<'a> {
    pub(crate) middlewares: &'a [Arc<dyn ErasedMiddleware>],
    pub(crate) transport: &'a dyn ErasedTransport,
    // The record of the engine's attempt; `None` for a chain made with `Next::to`.
    pub(crate) sends: Option<&'a Arc<Sends>>,
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
            sends: None,
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
        let zero_length = info.repeatable_body
            && request
                .headers()
                .get(http::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                == Some(0);
        let work = std::pin::pin!(if zero_length {
            futures_util::future::Either::Left(async {
                // HTTP/1 never polls a body declared empty. Validate factory output
                // after middleware admission, under the transport clocks.
                if let Some(body) = request.body_mut().take() {
                    let stream = reqwest::Response::from(http::Response::new(body)).bytes_stream();
                    let mut stream = std::pin::pin!(stream);
                    while let Some(piece) = stream.next().await {
                        let piece = piece.map_err(TransportError::from)?;
                        if !piece.is_empty() {
                            return Err(TransportError::upload(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "the content-length does not match the body length",
                            )));
                        }
                    }
                    // PANIC-SAFETY: This is a dereference assignment, not integer multiplication.
                    *request.body_mut() = Some(bytes::Bytes::new().into());
                }
                if run.expired() {
                    return Err(TransportError::clock(run.total_timeout()));
                }
                let last = *progress
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if last
                    .checked_add(run.idle())
                    .is_some_and(|end| Instant::now() >= end)
                {
                    return Err(TransportError::clock(run.idle_timeout()));
                }
                self.transport.send(request, info).await
            })
        } else {
            // Keep construction and ownership of ordinary transport work unchanged.
            futures_util::future::Either::Right(self.transport.send(request, info))
        });
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
    ///
    /// The send is recorded for its attempt from the moment this method is called, not from
    /// its first poll: in the record that the engine bound to this chain, and in the one that
    /// `info` carries, when they differ (a chain made with `Next::to` has none of its own).
    /// The layers below receive `info` bound to the engine's record again, whatever `TryInfo`
    /// was passed. See [`Error::request_not_sent`](crate::Error::request_not_sent).
    pub fn send(
        self,
        request: reqwest::Request,
        info: TryInfo,
    ) -> impl Future<Output = Result<reqwest::Response, TransportError>> + Send + 'a {
        let carried = info.sends();
        let (info, bound) = match self.sends {
            Some(record) => (info.bound_to(record), Some(record.clone())),
            None => (info, None),
        };
        // The record of the metadata, when it is not the bound one: `Next::to`, or a
        // `TryInfo` of another attempt.
        let carried = carried.filter(|carried| {
            bound
                .as_ref()
                .is_none_or(|bound| !Arc::ptr_eq(bound, carried))
        });
        let sending = match self.middlewares.split_first() {
            Some((middleware, rest)) => middleware.handle(
                request,
                info,
                Next {
                    middlewares: rest,
                    ..self
                },
            ),
            None => Box::pin(self.send_transport(request, info)),
        };
        crate::transport::counted(bound, crate::transport::counted(carried, sending))
    }
}
