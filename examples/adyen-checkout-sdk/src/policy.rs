use crate::{
    CaptureReceived, CaptureRequest, ERROR_LIMIT, Error, GUARANTEE_WINDOW, HttpClass,
    MethodsResponse, PaymentResponse, Problem, ProviderFailure, REQUEST_LIMIT, Reconciliation,
    ReconciliationReason as Reason,
};
use concord::{Endpoint, Json, Received, SendWith, field, reqwest};
use concord_retry::{Decision, Policy, Replay, Report, Retry, StopReason};
use std::{
    collections::BTreeMap,
    fmt,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};
use tokio::time::Instant;

/// Successful document with its authoritative response head and application attempt count.
pub struct Response<T> {
    /// Typed provider document; HTTP success alone is not authorisation.
    pub value: T,
    /// Response metadata; header values require explicit access.
    pub meta: concord::ResponseMeta,
    /// Application attempts and backoff accounting.
    pub retry: Report,
}

impl<T> fmt::Debug for Response<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Response")
            .field("value", &"<redacted>")
            .field("meta", &self.meta)
            .field("retry", &self.retry)
            .finish()
    }
}

/// A decoded success document checked against the exact request the policy sent.
///
/// Every output of the generated `send()` implements this. `false` means the document does
/// not describe that request (for example, the same key was used elsewhere in the company for a
/// different capture): the policy revokes the key and reports
/// [`GuaranteeNotConfirmed`](crate::ReconciliationReason::GuaranteeNotConfirmed).
pub trait Confirm {
    /// Whether this document describes `sent`, the request the policy dispatched.
    fn confirms(&self, sent: &reqwest::Request) -> bool;
}

impl Confirm for MethodsResponse {
    /// A methods list does not echo the request.
    fn confirms(&self, _: &reqwest::Request) -> bool {
        true
    }
}

impl Confirm for PaymentResponse {
    /// The payment and details contract compares no echoed fields (unchanged by capture).
    fn confirms(&self, _: &reqwest::Request) -> bool {
        true
    }
}

impl Confirm for CaptureReceived {
    /// The path's payment reference, merchant account, amount and reference must all match.
    fn confirms(&self, sent: &reqwest::Request) -> bool {
        let Some(Ok(request)) = sent
            .body()
            .and_then(|b| b.as_bytes())
            .map(serde_json::from_slice::<CaptureRequest>)
        else {
            return false;
        };
        // `.../payments/{paymentPspReference}/captures`. Concord percent-encodes path values, so
        // a reference that needed encoding never equals its segment: it fails closed.
        let mut segments = sent.url().path().rsplit('/');
        segments.next() == Some("captures")
            && segments.next() == Some(self.payment_psp_reference.as_str())
            && request.merchant_account == self.merchant_account
            && request.amount == self.requested
            && request.reference == self.reference
    }
}

#[derive(PartialEq, Eq)]
struct Identity {
    method: reqwest::Method,
    url: reqwest::Url,
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}
struct Entry {
    identity: Identity,
    first: Instant,
    first_wall: SystemTime,
    revoked: bool,
    active: bool,
}
type Ledger = Arc<Mutex<BTreeMap<String, Entry>>>;

/// Shared, bounded, process-local idempotency ledger and one retry owner.
///
/// No entry is evicted, including expired entries. Persist original keys and requests in the
/// application; creating a new policy or restarting the process cannot detect earlier use.
/// Share this instance across credentials in one company, and keep one regional destination.
#[derive(Clone)]
pub struct SendPolicy {
    ledger: Ledger,
    capacity: NonZeroUsize,
    retry: Policy,
    window: Duration,
}

impl fmt::Debug for SendPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SendPolicy(<redacted>)")
    }
}

impl Default for SendPolicy {
    fn default() -> Self {
        Self::new(NonZeroUsize::new(128).expect("nonzero constant"))
    }
}

impl SendPolicy {
    /// Creates a ledger with mandatory finite capacity (each request is at most REQUEST_LIMIT).
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            ledger: Arc::default(),
            capacity,
            retry: Policy {
                timeout: Some(Duration::from_secs(10)),
                initial_delay: Duration::from_millis(100),
                max_delay: Duration::from_secs(2),
                ..Policy::new(NonZeroUsize::new(3).expect("nonzero constant"))
            },
            window: GUARANTEE_WINDOW,
        }
    }

    /// Chooses attempt/deadline/backoff bounds. A finite nonzero deadline is required.
    /// Deterministic capped exponential backoff is used; there is no random jitter source.
    pub fn with_retry(mut self, policy: Policy) -> Result<Self, Error> {
        if policy.timeout.is_none_or(|d| d.is_zero()) {
            return Err(Error::InvalidInput(
                "retry requires a finite nonzero timeout",
            ));
        }
        self.retry = policy;
        Ok(self)
    }

    /// Shortens the guarantee window (for stricter application policy or local fixture tests).
    /// Cannot extend Adyen's minimum retention. Existing clones keep their own shorter bound.
    pub fn with_guarantee_window(mut self, window: Duration) -> Result<Self, Error> {
        if window.is_zero() || window > self.window {
            return Err(Error::InvalidInput(
                "guarantee window must be positive and may only shorten",
            ));
        }
        self.window = window;
        Ok(self)
    }

    /// Records external knowledge that a previously submitted key no longer has a guarantee.
    /// Unknown keys are rejected rather than creating a fresh ledger entry.
    pub fn revoke(&self, key: &str) -> Result<(), Error> {
        let mut ledger = self.ledger.lock().map_err(|_| Error::PolicyUnavailable)?;
        let entry = ledger
            .get_mut(key)
            .ok_or(Error::InvalidInput("key is not registered"))?;
        entry.revoked = true;
        Ok(())
    }

    fn reserve(&self, key: String, identity: Identity) -> Result<Reservation, Error> {
        let mut ledger = self.ledger.lock().map_err(|_| Error::PolicyUnavailable)?;
        match ledger.get_mut(&key) {
            Some(entry) => {
                if entry.identity != identity {
                    return Err(Error::reconcile(Reason::ChangedRequest));
                }
                check(entry, self.window)?;
                if entry.active {
                    return Err(Error::reconcile(Reason::InFlight));
                }
                entry.active = true;
            }
            None => {
                if ledger.len() >= self.capacity.get() {
                    return Err(Error::LedgerFull);
                }
                ledger.insert(
                    key.clone(),
                    Entry {
                        identity,
                        first: Instant::now(),
                        first_wall: SystemTime::now(),
                        revoked: false,
                        active: true,
                    },
                );
            }
        }
        Ok(Reservation {
            ledger: self.ledger.clone(),
            key,
        })
    }
}

struct Reservation {
    ledger: Ledger,
    key: String,
}
impl Reservation {
    fn check(&self, window: Duration) -> Result<(), Error> {
        let ledger = self.ledger.lock().map_err(|_| Error::PolicyUnavailable)?;
        check(
            ledger.get(&self.key).expect("entries never removed"),
            window,
        )
    }
    fn revoke(&self) {
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger
                .get_mut(&self.key)
                .expect("entries never removed")
                .revoked = true;
        }
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        if let Ok(mut ledger) = self.ledger.lock() {
            ledger
                .get_mut(&self.key)
                .expect("entries never removed")
                .active = false;
        }
    }
}
fn check(entry: &Entry, window: Duration) -> Result<(), Error> {
    if entry.revoked {
        Err(Error::reconcile(Reason::GuaranteeRevoked))
    } else if Instant::now().saturating_duration_since(entry.first) >= window
        || !SystemTime::now()
            .duration_since(entry.first_wall)
            .is_ok_and(|age| age < window)
    {
        // Wall time catches suspension that some monotonic clocks omit. Clock rollback
        // makes age untrustworthy, so it also refuses replay rather than extending retention.
        Err(Error::reconcile(Reason::GuaranteeExpired))
    } else {
        Ok(())
    }
}

fn assurance_lost<T>(received: &Received<T>, key: &str) -> bool {
    match field::single(received.head().headers(), "idempotency-key") {
        Ok(Some(value)) => value.as_bytes() != key.as_bytes(),
        Ok(None) => received.status().is_success(),
        Err(_) => true,
    }
}

fn transient<T>(received: &Received<T>) -> Option<Duration> {
    // Missing, false, repeated and malformed transient-error never grant replay permission.
    if received.status().is_success()
        || field::single(received.head().headers(), "transient-error")
            .ok()
            .flatten()
            .is_none_or(|v| v.as_bytes() != b"true")
    {
        return None;
    }
    concord_retry::http::retry_after(
        received
            .head()
            .headers()
            .get_all("retry-after")
            .iter()
            .map(|v| v.as_bytes()),
        SystemTime::now(),
    )
    .ok()
    .map(|delay| delay.unwrap_or_default())
}

impl<T: Send + Confirm> SendWith<T> for SendPolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?;
        let request = prepared.request();
        let url = request.url();
        let sandbox = url.scheme() == "https"
            && url.host_str() == Some("checkout-test.adyen.com")
            && url.port_or_known_default() == Some(443);
        let loopback =
            url.scheme() == "http" && matches!(url.host_str(), Some("127.0.0.1" | "[::1]"));
        if !sandbox && !loopback {
            return Err(Error::InvalidInput(
                "destination must be Adyen test or HTTP loopback",
            ));
        }
        let key = field::single(request.headers(), "idempotency-key")
            .ok()
            .flatten()
            .and_then(|v| v.to_str().ok())
            .filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_graphic()))
            .ok_or(Error::InvalidInput(
                "idempotency key must be 1..64 visible ASCII characters",
            ))?
            .to_owned();
        let body = request
            .body()
            .and_then(|b| b.as_bytes())
            .ok_or(Error::InvalidInput(
                "only fixed JSON request bodies can be replayed",
            ))?;
        if body.len() > REQUEST_LIMIT {
            return Err(Error::InvalidInput("encoded request exceeds SDK limit"));
        }
        let identity = Identity {
            method: request.method().clone(),
            url: request.url().clone(),
            headers: request.headers().clone(),
            body: body.to_vec(),
        };
        let reservation = self.reserve(key.clone(), identity)?;
        // Native decomposition permits a fallible guarantee check after every retry wait.
        // Retry::receive's hook cannot refuse dispatch; no bytes or headers are modified here.
        let (client, template, decoder) = prepared.into_parts();
        let epoch = Instant::now();
        let run = Retry::new(self.retry, Replay::Permitted)
            .run(
                |_| {
                    let request = template.try_clone().expect("fixed JSON body is cloneable");
                    let reservation = &reservation;
                    let client = &client;
                    let decoder = &decoder;
                    async move {
                        reservation.check(self.window)?;
                        decoder.execute(client, request).await.map_err(Error::from)
                    }
                },
                |outcome| match outcome {
                    Err(Error::Concord(e)) if e.kind() == concord::ErrorKind::Transport => {
                        Decision::RetryAfter(None)
                    }
                    Ok(received) if assurance_lost(received, &key) => Decision::Done,
                    Ok(received) => transient(received)
                        .map_or(Decision::Done, |d| Decision::RetryAfter(Some(d))),
                    _ => Decision::Done,
                },
                || epoch.elapsed(),
                tokio::time::sleep,
                |_, ceiling| ceiling,
            )
            .await;
        let retry = run.report;
        if retry.stop == StopReason::Deadline {
            let (meta, cause) = match run.outcome {
                Some(Ok(received)) => (Some(received.meta()), None),
                Some(Err(Error::Concord(cause))) => (None, Some(cause)),
                _ => (None, None),
            };
            return Err(Error::Reconciliation(Box::new(Reconciliation {
                reason: Reason::Deadline,
                meta,
                provider: None,
                cause,
                retry: Some(retry),
            })));
        }
        let received = match run
            .outcome
            .expect("finite nonzero deadline starts an attempt")
        {
            Ok(received) => received,
            Err(Error::Concord(cause)) => {
                return Err(Error::Reconciliation(Box::new(Reconciliation {
                    reason: Reason::UncertainDelivery,
                    meta: None,
                    provider: None,
                    cause: Some(cause),
                    retry: Some(retry),
                })));
            }
            Err(mut error) => {
                if let Error::Reconciliation(e) = &mut error {
                    e.retry = Some(retry);
                }
                return Err(error);
            }
        };
        if assurance_lost(&received, &key) {
            reservation.revoke();
            return Err(Error::Reconciliation(Box::new(Reconciliation {
                reason: Reason::GuaranteeNotConfirmed,
                meta: Some(received.meta()),
                provider: None,
                cause: None,
                retry: Some(retry),
            })));
        }
        if !received.status().is_success() {
            let is_transient = transient(&received).is_some();
            let (meta, document) = received.document::<Json, Problem>(ERROR_LIMIT).await;
            let reason = document
                .as_ref()
                .ok()
                .and_then(|p| match p.error_code.as_str() {
                    "703" => Some(Reason::GuaranteeUnavailable),
                    "704" => Some(Reason::InProgress),
                    _ => None,
                });
            let failure = Box::new(ProviderFailure {
                class: HttpClass::of(meta.status().as_u16()),
                meta,
                document: document.map_err(Box::new),
                transient: is_transient,
                retry: retry.clone(),
            });
            if let Some(reason) = reason {
                if reason == Reason::GuaranteeUnavailable {
                    reservation.revoke();
                }
                return Err(Error::Reconciliation(Box::new(Reconciliation {
                    reason,
                    meta: None,
                    provider: Some(failure),
                    cause: None,
                    retry: Some(retry),
                })));
            }
            return Err(Error::Provider(failure));
        }
        let (value, meta) = received.decode_with_meta().await.map_err(|cause| {
            Error::Reconciliation(Box::new(Reconciliation {
                reason: Reason::UncertainDelivery,
                meta: None,
                provider: None,
                cause: Some(Box::new(cause)),
                retry: Some(retry.clone()),
            }))
        })?;
        if !value.confirms(&template) {
            // The key's provider record answers some other request; replaying it cannot submit
            // this one, so the guarantee is treated like a mismatched key echo.
            reservation.revoke();
            return Err(Error::Reconciliation(Box::new(Reconciliation {
                reason: Reason::GuaranteeNotConfirmed,
                meta: Some(meta),
                provider: None,
                cause: None,
                retry: Some(retry),
            })));
        }
        Ok(Response { value, meta, retry })
    }
}
