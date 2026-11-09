//! The client's send policy ([`Session`]): what every generated `send()` does.

use std::fmt;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord::reqwest::header::AUTHORIZATION;
use concord::reqwest::{self, StatusCode, Url};
use concord::{Endpoint, Received, SendOperation};
use concord_auth::{Generation, RefreshError, Snapshot, TokenCache};
use concord_retry::{Decision, Replay, Retry, concord::ReplayMode};

use crate::auth::{OAuth, fetch_token};
use crate::{
    Credentials, Error, HttpError, PayPalOperation, RequestId, TokenError, Uncertain,
    UncertainCause,
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

/// Before each attempt, the send policy replaces `authorization` with the current token
/// snapshot (`Bearer <access_token>`). Generated `prepare()` does not acquire or apply this token.
///
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
/// - **Writes** (create, order authorize/capture, authorization capture/void) are repeated with
///   the same body and `PayPal-Request-Id`, only when they carry a [`RequestId`]: after a transport failure, a
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

/// Every PayPal rule the policy keys on the operation.
struct Class {
    /// A write: sent within the write bounds, and possibly applied when unanswered.
    write: bool,
    /// The endpoint's reference documents `PayPal-Request-Id`: the only writes that may be
    /// repeated, and only when the header is present.
    accepts_request_id: bool,
}

impl Class {
    /// No wildcard arm: an endpoint added to the declaration without a class here does not
    /// compile (E0004, naming its `PayPalOperation` variant).
    fn of(operation: PayPalOperation) -> Self {
        use PayPalOperation as Op;
        match operation {
            Op::OrderGet | Op::OrderGetAuthorized | Op::CaptureGet => Self {
                write: false,
                accepts_request_id: false,
            },
            Op::OrdersCreate
            | Op::OrderAuthorize
            | Op::OrderCapture
            | Op::AuthorizationCapture
            | Op::AuthorizationVoid => Self {
                write: true,
                accepts_request_id: true,
            },
        }
    }
}

#[derive(Default)]
struct Observed {
    /// An attempt that may have been performed without a usable answer.
    ambiguous: bool,
    /// A `401` already led to a new token.
    renewed: bool,
}

fn classify<T>(
    outcome: &Result<Received<T>, concord::Error>,
    generation: &Generation,
    seen: &mut Observed,
    cache: &TokenCache<(), TokenError>,
) -> Decision {
    match outcome {
        Err(_) => Decision::RetryAfter(None),
        Ok(received) => {
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

impl<T: Send + 'static> SendOperation<T, PayPalOperation> for Session {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(
        &self,
        operation: PayPalOperation,
        endpoint: E,
    ) -> Self::Output {
        let Class {
            write,
            accepts_request_id,
        } = Class::of(operation);
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
        let permitted = !write || (request_id.is_some() && accepts_request_id);

        let replay = if permitted {
            Replay::Permitted
        } else {
            Replay::Never
        };
        let bounds = if write {
            self.inner.writes
        } else {
            self.inner.reads
        };
        let mut seen = Observed::default();
        let cache = &self.inner.cache;
        let epoch = Instant::now();
        let run = Retry::new(bounds, replay)
            .receive_with(
                prepared,
                ReplayMode::BestEffort,
                |mut request, _| async move {
                    let token = self.token().await?;
                    request
                        .headers_mut()
                        .insert(AUTHORIZATION, token.header().clone());
                    Ok::<_, TokenError>((request, token.generation()))
                },
                |outcome, generation| classify(outcome, generation, &mut seen, cache),
                move || epoch.elapsed(),
                tokio::time::sleep,
                |_, ceiling| ceiling,
            )
            .await;
        let attempts = run.report.retry.attempts;
        seen.ambiguous |= run.report.no_response;
        let name = E::operation().name();

        if !write {
            if let Some(e) = run.refusal {
                return Err(Error::Token(e));
            }
            return match run.outcome {
                None => Err(Error::Deadline),
                Some(Err(e)) => Err(Error::Concord(e)),
                Some(Ok(received)) if received.status().is_success() => {
                    Ok(received.decode().await?)
                }
                Some(Ok(received)) => Err(Error::Http(Box::new(
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
        if attempts > run.report.completed {
            // An attempt was cancelled by the deadline after it may have been sent.
            return Err(uncertain(UncertainCause::Interrupted));
        }
        if let Some(e) = run.refusal {
            return Err(if seen.ambiguous {
                uncertain(UncertainCause::LaterToken(e))
            } else {
                Error::Token(e)
            });
        }
        match run.outcome {
            None => Err(Error::Deadline),
            Some(Err(e)) => Err(uncertain(UncertainCause::NoResponse(e))),
            Some(Ok(received)) if received.status().is_success() => received
                .decode()
                .await
                .map_err(|e| uncertain(UncertainCause::UnreadableSuccess(e))),
            Some(Ok(received)) => {
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
