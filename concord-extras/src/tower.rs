//! A `tower` service used as a transport. Requires the `tower` feature.

use concord_core::error::Class;
use concord_core::reqwest;
use concord_core::transport::{Transport, TransportError, TryInfo};

/// Turns a `tower` service into a transport, so that its layers (a rate limit, a limit on
/// concurrent requests) sit under a Concord client.
///
/// `poll_ready` and `&mut self` stay inside this adapter. There is one service: Concord holds it
/// while it gets ready and is called, one request at a time, and awaits the response outside
/// the lock. So a layer that keeps state and cannot be cloned (a rate limit) works as it is.
/// `reqwest::Client` is already a `tower` service, so it can be the bottom of the stack.
///
/// An error from a layer that Concord does not know has the class Other: behind a timeout
/// layer, `is_timeout()` is false. Map the error in the stack:
/// `.map_err(|_| TransportError::timeout())`.
///
/// A stack built with `tower::ServiceBuilder` and its `limit` feature can wrap
/// `reqwest::Client`. Pass the finished stack to [`Tower::new`], then pass the
/// adapter to the generated client's `with_transport` method.
///
/// A small service can also return a response built in memory:
///
/// ```
/// # mod concord { pub use concord_core::{reply, reqwest, transport}; pub use concord_extras::tower; }
/// use concord::{reqwest, transport::{Transport, TransportError, TryInfo}};
/// use concord::tower::Tower;
/// use std::{future::{ready, Ready}, task::{Context, Poll}};
/// struct Hello;
/// impl tower_service::Service<reqwest::Request> for Hello {
///     type Response = reqwest::Response;
///     type Error = TransportError;
///     type Future = Ready<Result<Self::Response, Self::Error>>;
///     fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
///         Poll::Ready(Ok(()))
///     }
///     fn call(&mut self, _: reqwest::Request) -> Self::Future {
///         ready(Ok(concord::reply(200, "hello")))
///     }
/// }
/// # tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
/// let transport = Tower::new(Hello);
/// let request = reqwest::Request::new(reqwest::Method::GET, "https://example.test/".parse()?);
/// let response = transport.send(request, TryInfo::new("hello.get", 1)).await?;
/// assert_eq!(response.bytes().await?, "hello");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// # }).unwrap();
/// ```
pub struct Tower<S> {
    service: futures_util::lock::Mutex<S>,
}

impl<S> Tower<S> {
    /// Creates a transport from this service.
    pub fn new(service: S) -> Tower<S> {
        Tower {
            service: futures_util::lock::Mutex::new(service),
        }
    }
}

impl<S> std::fmt::Debug for Tower<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tower").finish_non_exhaustive()
    }
}

/// How an error of the stack becomes a class: a [`TransportError`] stays as it is; a
/// `reqwest::Error` is classified as `From<reqwest::Error>` classifies it (so a failed
/// connection at the bottom is still Connect); anything else is Other. A layer of `tower-http`
/// takes an `http::Request`, not a `reqwest::Request`, so it does not fit under this adapter.
impl<S> Transport for Tower<S>
where
    S: tower_service::Service<reqwest::Request, Response = reqwest::Response> + Send + 'static,
    S::Future: Send,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    async fn send(
        &self,
        request: reqwest::Request,
        _info: TryInfo,
    ) -> Result<reqwest::Response, TransportError> {
        let url = request.url().clone();
        let answer = {
            let mut service = self.service.lock().await;
            std::future::poll_fn(|cx| service.poll_ready(cx))
                .await
                .map_err(classify)?;
            service.call(request)
        };
        answer
            .await
            .map_err(classify)
            .map(|response| concord_core::transport::__response_at(response, url))
    }
}

fn classify(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> TransportError {
    let error = error.into();
    match error.downcast::<TransportError>() {
        Ok(same) => *same,
        Err(error) => match error.downcast::<reqwest::Error>() {
            Ok(http) => TransportError::from(*http),
            Err(error) => TransportError::new(Class::Other).with_source(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{task::noop_waker_ref, FutureExt};
    use std::future::{Future, Ready};
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll};

    fn request() -> reqwest::Request {
        reqwest::Request::new(
            reqwest::Method::GET,
            "https://example.test/items".parse().unwrap(),
        )
    }

    fn info() -> TryInfo {
        TryInfo::new("items.get", 1)
    }

    fn pending(future: Pin<&mut impl Future>) -> bool {
        future
            .poll(&mut Context::from_waker(noop_waker_ref()))
            .is_pending()
    }

    // Deliberately neither Clone nor Sync, like a stateful rate limiter.
    struct Stateful {
        ready: Arc<AtomicBool>,
        calls: Arc<AtomicUsize>,
        permit: bool,
        local: std::cell::Cell<usize>,
    }

    impl tower_service::Service<reqwest::Request> for Stateful {
        type Response = reqwest::Response;
        type Error = TransportError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            if self.ready.load(Ordering::SeqCst) {
                self.permit = true;
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }

        fn call(&mut self, request: reqwest::Request) -> Self::Future {
            assert!(std::mem::take(&mut self.permit), "call requires readiness");
            assert_eq!(request.url().path(), "/items");
            self.local.set(self.local.get() + 1);
            let number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
            assert_eq!(self.local.get(), number, "the same service keeps its state");
            Box::pin(async move {
                if number == 1 {
                    std::future::pending::<()>().await;
                }
                Ok(concord_core::reply(200, number.to_string()))
            })
        }
    }

    #[test]
    fn c9_1_tower_waits_for_readiness_and_unlocks_before_awaiting_the_answer() {
        let ready = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = Tower::new(Stateful {
            ready: ready.clone(),
            calls: calls.clone(),
            permit: false,
            local: std::cell::Cell::new(0),
        });
        let mut first = Box::pin(transport.send(request(), info()));
        let mut second = Box::pin(transport.send(request(), info()));
        assert!(pending(first.as_mut()));
        assert!(pending(second.as_mut()));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        ready.store(true, Ordering::SeqCst);
        assert!(pending(first.as_mut()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let response = second.now_or_never().unwrap().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(response.bytes().now_or_never().unwrap().unwrap(), "2");
        drop(first);
    }

    #[test]
    fn c9_1_tower_cancelling_readiness_releases_the_service_for_the_next_send() {
        let ready = Arc::new(AtomicBool::new(false));
        let transport = Tower::new(Stateful {
            ready: ready.clone(),
            calls: Arc::new(AtomicUsize::new(0)),
            permit: false,
            local: std::cell::Cell::new(0),
        });
        let mut first = Box::pin(transport.send(request(), info()));
        let mut second = Box::pin(transport.send(request(), info()));
        assert!(pending(first.as_mut()));
        assert!(pending(second.as_mut()));
        drop(first);
        ready.store(true, Ordering::SeqCst);
        assert!(pending(second.as_mut())); // The first response is intentionally pending.
        assert!(transport.service.try_lock().is_some());
    }

    struct Failing<E> {
        before_call: bool,
        error: Option<E>,
        calls: Arc<AtomicUsize>,
    }

    impl<E> tower_service::Service<reqwest::Request> for Failing<E> {
        type Response = reqwest::Response;
        type Error = E;
        type Future = Ready<Result<Self::Response, E>>;

        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), E>> {
            if self.before_call {
                Poll::Ready(Err(self.error.take().unwrap()))
            } else {
                Poll::Ready(Ok(()))
            }
        }

        fn call(&mut self, _: reqwest::Request) -> Self::Future {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(Err(self.error.take().unwrap()))
        }
    }

    fn failure<E>(before_call: bool, error: E) -> TransportError
    where
        E: Into<Box<dyn std::error::Error + Send + Sync>> + Send + 'static,
    {
        let calls = Arc::new(AtomicUsize::new(0));
        let transport = Tower::new(Failing {
            before_call,
            error: Some(error),
            calls: calls.clone(),
        });
        let error = transport
            .send(request(), info())
            .now_or_never()
            .unwrap()
            .unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), usize::from(!before_call));
        assert!(transport.service.try_lock().is_some());
        error
    }

    #[tokio::test]
    async fn c9_1_tower_classifies_both_readiness_and_call_errors() {
        use std::error::Error as _;
        for before_call in [false, true] {
            for class in [Class::Connect, Class::Timeout, Class::Blocked, Class::Body] {
                let error = failure(
                    before_call,
                    TransportError::new(class).with_source(std::io::Error::other("layer cause")),
                );
                assert_eq!(error.class(), class);
                assert_eq!(error.source().unwrap().to_string(), "layer cause");
            }
            let error = failure(before_call, std::io::Error::other("unknown layer"));
            assert_eq!(error.class(), Class::Other);
            assert_eq!(error.source().unwrap().to_string(), "unknown layer");

            let http = reqwest::Client::new()
                .get("https://private.example.test/secret")
                .header("x-test", "\n")
                .build()
                .unwrap_err();
            let error = failure(before_call, http);
            assert_eq!(error.class(), Class::Other);
            let source = error
                .source()
                .unwrap()
                .downcast_ref::<reqwest::Error>()
                .unwrap();
            assert!(source.url().is_none());

            // Reqwest rejects HTTP/0.9 before opening a socket.
            let mut request = request();
            *request.version_mut() = concord_core::http::Version::HTTP_09;
            let http = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .execute(request)
                .await
                .unwrap_err();
            assert!(http.is_request());
            let error = failure(before_call, http);
            assert_eq!(error.class(), Class::Connect);
            assert!(error
                .source()
                .unwrap()
                .downcast_ref::<reqwest::Error>()
                .unwrap()
                .url()
                .is_none());
        }
    }

    #[test]
    fn a_reqwest_client_is_a_service_and_so_a_transport() {
        fn transport<T: concord_core::Transport>() {}
        transport::<super::Tower<concord_core::reqwest::Client>>();
        let _ = |http: concord_core::reqwest::Client| super::Tower::new(http);
    }
    #[test]
    fn c8_6_tower_attaches_the_request_url_to_a_plain_response_and_keeps_a_real_url() {
        use reqwest::ResponseBuilderExt;
        struct Replies(bool);
        impl tower_service::Service<reqwest::Request> for Replies {
            type Response = reqwest::Response;
            type Error = TransportError;
            type Future = Ready<Result<Self::Response, Self::Error>>;
            fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, _: reqwest::Request) -> Self::Future {
                let response = concord_core::http::Response::builder()
                    .status(201)
                    .header("link", "<?page=2>; rel=\"next\"");
                let response = if self.0 {
                    response.url("https://example.test/redirected".parse().unwrap())
                } else {
                    response
                };
                std::future::ready(Ok(response.body("body").unwrap().into()))
            }
        }
        for real in [false, true] {
            let transport = Tower::new(Replies(real));
            let response = transport
                .send(request(), info())
                .now_or_never()
                .unwrap()
                .unwrap();
            assert_eq!(
                response.url().as_str(),
                if real {
                    "https://example.test/redirected"
                } else {
                    "https://example.test/items"
                }
            );
            assert_eq!(response.status(), 201);
            assert_eq!(response.headers()["link"], "<?page=2>; rel=\"next\"");
            assert_eq!(response.bytes().now_or_never().unwrap().unwrap(), "body");
        }
    }
}
