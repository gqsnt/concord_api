//! The client's send policy ([`Session`]): what every generated `send()` does.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord::reqwest::header::AUTHORIZATION;
use concord::reqwest::{self, StatusCode, Url};
use concord::{Delivery, Endpoint, Operation, Received, SendWith};
use concord_auth::{Generation, RefreshError, Snapshot, TokenCache};
use concord_retry::{Decision, Replay, Retry};

use crate::auth::{OAuth, fetch_token};
use crate::{
    Credentials, Error, HttpError, Order, PayPalOrderCapture, PayPalOrdersCreate, RequestId,
    TokenError, Uncertain, UncertainCause,
};

pub(crate) fn default_reads() -> concord_retry::Policy {
    concord_retry::Policy {
        initial_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(4),
        ..concord_retry::Policy::new(NonZeroUsize::new(3).expect("nonzero"))
    }
}

pub(crate) fn default_writes() -> concord_retry::Policy {
    concord_retry::Policy {
        initial_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(4),
        ..concord_retry::Policy::new(NonZeroUsize::new(2).expect("nonzero"))
    }
}

/// The policy behind every generated `send()` of [`PayPal`](crate::PayPal).
///
/// - **Token.** Before each attempt it takes the cached access token, or obtains one with the
///   client-credentials grant: one token request at a time for all clones, concurrent callers
///   waiting for the same one; a token is replaced `refresh margin` before `expires_in` runs
///   out. A failed acquisition is not cached: the next call tries again. There is no refresh
///   token. A `401` invalidates the token that request used.
/// - **Destination.** The token and every request go to the origin given at construction;
///   a request whose URL has another origin is refused before anything is sent.
/// - **Reads** (`GET`) are repeated after a transport failure or a `5xx`, and once after a
///   `401` with a new token, within the read bounds. Other statuses (a `429` included: PayPal
///   publishes no quota) are returned at once.
/// - **Writes** (Create order, Capture payment for order) are repeated, with the same body and
///   `PayPal-Request-Id`, only when they carry a [`RequestId`]: after a transport failure, a
///   `5xx`, or once after a `401`. Without one, a write is sent exactly once. A write that may
///   have been applied — no response, a `5xx`, an unreadable `2xx`, or any refusal after such an
///   attempt — is [`Error::Uncertain`]; a `4xx` on a first attempt is [`Error::Http`].
///
/// `prepare()` bypasses the policy and returns the declared request without `Authorization`.
#[derive(Clone)]
pub struct Session {
    inner: Arc<Inner>,
}

struct Inner {
    oauth: OAuth,
    base: Url,
    cache: TokenCache<(), TokenError>,
    margin: Duration,
    reads: concord_retry::Policy,
    writes: concord_retry::Policy,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("origin", &self.inner.base.origin().ascii_serialization())
            .field("credentials", &"<redacted>")
            .field("reads", &self.inner.reads)
            .field("writes", &self.inner.writes)
            .finish_non_exhaustive()
    }
}

impl Session {
    pub(crate) fn new(
        http: reqwest::Client,
        credentials: Credentials,
        base: &str,
        reads: concord_retry::Policy,
        writes: concord_retry::Policy,
        margin: Duration,
        clock: Option<Box<dyn Fn() -> Instant + Send + Sync + 'static>>,
    ) -> Result<Self, Error> {
        let oauth = OAuth::for_base(http, credentials, base)?;
        let base = Url::parse(base).map_err(|_| Error::Invalid("base URL is not a URL"))?;
        let capacity = NonZeroUsize::new(1).expect("nonzero");
        let cache = match clock {
            Some(clock) => TokenCache::with_clock(capacity, margin, clock),
            None => TokenCache::new(capacity, margin),
        };
        Ok(Self {
            inner: Arc::new(Inner {
                oauth,
                base,
                cache,
                margin,
                reads,
                writes,
            }),
        })
    }

    async fn token(&self) -> Result<Snapshot, TokenError> {
        let oauth = self.inner.oauth.clone();
        let margin = self.inner.margin;
        self.inner
            .cache
            .get_or_refresh((), move || fetch_token(oauth, margin))
            .await
            .map_err(|e| match e {
                RefreshError::Acquire(e) => (*e).clone(),
                RefreshError::LifetimeOverflow => {
                    TokenError::Unusable("expires_in exceeds the clock's range")
                }
                RefreshError::Capacity => TokenError::Unusable("token cache is occupied"),
            })
    }
}

/// The endpoints whose reference documents `PayPal-Request-Id`: the only writes that may be
/// repeated, and only when the header is present.
fn accepts_request_id(operation: &Operation) -> bool {
    *operation == <PayPalOrdersCreate as Endpoint<Order>>::operation()
        || *operation == <PayPalOrderCapture as Endpoint<Order>>::operation()
}

/// One attempt: a token failure (nothing sent) or one execution with the token it used.
enum Attempt<T> {
    Token(TokenError),
    Sent(Result<Received<T>, concord::Error>, Generation),
}

#[derive(Default)]
struct Observed {
    /// Completed attempts (the engine also counts one cancelled at the deadline).
    completed: usize,
    /// An attempt that may have been performed without a usable answer.
    ambiguous: bool,
    /// A `401` already led to a new token.
    renewed: bool,
}

fn classify<T>(
    attempt: &Attempt<T>,
    seen: &mut Observed,
    cache: &TokenCache<(), TokenError>,
) -> Decision {
    seen.completed += 1;
    match attempt {
        // The request was not sent; a token failure is final for this call.
        Attempt::Token(_) => Decision::Stop,
        Attempt::Sent(Err(e), _) => {
            seen.ambiguous |= e.delivery() != Delivery::NotSent;
            Decision::RetryAfter(None)
        }
        Attempt::Sent(Ok(received), generation) => {
            let status = received.status();
            if status == StatusCode::UNAUTHORIZED {
                cache.invalidate(&(), generation);
                if seen.renewed {
                    return Decision::Done;
                }
                seen.renewed = true;
                return Decision::RetryAfter(None);
            }
            if status.is_server_error() {
                seen.ambiguous = true;
                return Decision::RetryAfter(None);
            }
            Decision::Done
        }
    }
}

impl<T: Send + 'static> SendWith<T> for Session {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let operation = E::operation();
        let prepared = endpoint.prepare()?;
        if prepared.request().url().origin() != self.inner.base.origin() {
            return Err(Error::Destination);
        }
        let request_id = prepared
            .request()
            .headers()
            .get("paypal-request-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| RequestId::new(v).ok());
        let write = operation.method() != Some("GET");
        let permitted = !write || (request_id.is_some() && accepts_request_id(&operation));

        let (client, template, decoder) = prepared.into_parts();
        // Every body of this declaration is buffered JSON, so the template can be cloned.
        let replay = if permitted && template.try_clone().is_some() {
            Replay::Permitted
        } else {
            Replay::Never
        };
        let bounds = if write {
            self.inner.writes
        } else {
            self.inner.reads
        };
        let mut template = Some(template);
        let mut seen = Observed::default();
        let cache = &self.inner.cache;
        let client = &client;
        let epoch = Instant::now();
        let run = Retry::new(bounds, replay)
            .run(
                |_| {
                    let request = template
                        .as_ref()
                        .and_then(reqwest::Request::try_clone)
                        .or_else(|| template.take());
                    async move {
                        let mut request =
                            request.expect("a request is sent at most once unless cloneable");
                        match self.token().await {
                            Err(e) => Attempt::Token(e),
                            Ok(token) => {
                                request
                                    .headers_mut()
                                    .insert(AUTHORIZATION, token.header().clone());
                                Attempt::Sent(
                                    decoder.execute(client, request).await,
                                    token.generation(),
                                )
                            }
                        }
                    }
                },
                |attempt| classify(attempt, &mut seen, cache),
                move || epoch.elapsed(),
                tokio::time::sleep,
                |_, ceiling| ceiling,
            )
            .await;
        let attempts = run.report.attempts;
        let name = operation.name();

        if !write {
            return match run.outcome {
                None => Err(Error::Deadline),
                Some(Attempt::Token(e)) => Err(Error::Token(e)),
                Some(Attempt::Sent(Err(e), _)) => Err(Error::Concord(e)),
                Some(Attempt::Sent(Ok(received), _)) if received.status().is_success() => {
                    Ok(received.decode().await?)
                }
                Some(Attempt::Sent(Ok(received), _)) => Err(Error::Http(Box::new(
                    HttpError::read(received, name, attempts).await,
                ))),
            };
        }

        let uncertain = |cause| {
            Error::Uncertain(Box::new(Uncertain {
                operation: name,
                request_id: request_id.clone(),
                attempts,
                cause,
            }))
        };
        if attempts > seen.completed {
            // An attempt was cancelled by the deadline after it may have been sent.
            return Err(uncertain(UncertainCause::Interrupted));
        }
        match run.outcome {
            None => Err(Error::Deadline),
            Some(Attempt::Token(e)) if seen.ambiguous => {
                Err(uncertain(UncertainCause::LaterToken(e)))
            }
            Some(Attempt::Token(e)) => Err(Error::Token(e)),
            Some(Attempt::Sent(Err(e), _)) => Err(uncertain(UncertainCause::NoResponse(e))),
            Some(Attempt::Sent(Ok(received), _)) if received.status().is_success() => received
                .decode()
                .await
                .map_err(|e| uncertain(UncertainCause::UnreadableSuccess(e))),
            Some(Attempt::Sent(Ok(received), _)) => {
                let server_error = received.status().is_server_error();
                let http = Box::new(HttpError::read(received, name, attempts).await);
                if server_error {
                    Err(uncertain(UncertainCause::ServerError(http)))
                } else if seen.ambiguous {
                    Err(uncertain(UncertainCause::LaterRefusal(http)))
                } else {
                    Err(Error::Http(http))
                }
            }
        }
    }
}
