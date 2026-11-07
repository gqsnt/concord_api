//! The client's send policy: what every generated `send()` does.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant, SystemTime};

use concord::reqwest::StatusCode;
use concord::reqwest::header::RETRY_AFTER;
use concord::{Delivery, Endpoint, Prepared, Received, SendWith};
use concord_retry::concord::ReplayMode;
use concord_retry::{Decision, Replay, Retry};

use crate::{Error, HttpError, IdempotencyKey, UncertainCause, UncertainWrite};

/// How generated `send()` treats reads and writes.
///
/// - **Reads** (`GET`: List Tickets, Show Ticket) are safe to repeat. A `429`, or a `503`
///   carrying `Retry-After`, is sent again after exactly the server's delay, at most
///   [`read_attempts`](Self::read_attempts) times in all, and only while that delay is at most
///   [`max_retry_after`](Self::max_retry_after). Anything else — another status, a missing or
///   invalid `Retry-After`, a transport failure — is returned at once. No local backoff is added.
/// - **Writes** (`POST` create and `PUT` update) are sent exactly once. A 4xx is an [`Error::Http`], except an update 409 is [`Error::Collision`]; a
///   possibly-applied write (no response after sending, a 5xx, an unreadable 2xx) is an
///   [`Error::Uncertain`] carrying the request's idempotency key when present. A `429` on a write is returned
///   with its `Retry-After`: the request was not processed, and the caller decides when to send
///   it again (with the same key for a create).
///
/// The policy is a client value (`send policy;`), replaced with
/// [`Zendesk::set_policy`](crate::Zendesk::set_policy). `prepare()` bypasses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SendPolicy {
    read_attempts: NonZeroUsize,
    max_retry_after: Duration,
    read_deadline: Option<Duration>,
}

impl Default for SendPolicy {
    /// Three attempts per read, server delays up to 60 seconds, no overall read deadline.
    fn default() -> Self {
        Self {
            read_attempts: NonZeroUsize::new(3).expect("nonzero"),
            max_retry_after: Duration::from_secs(60),
            read_deadline: None,
        }
    }
}

impl SendPolicy {
    /// Attempts per read, the first included (`1` disables honouring `Retry-After`).
    #[must_use]
    pub fn read_attempts(mut self, attempts: NonZeroUsize) -> Self {
        self.read_attempts = attempts;
        self
    }

    /// The longest `Retry-After` a read waits for; a longer one is returned as the error, with
    /// its delay, without waiting.
    #[must_use]
    pub fn max_retry_after(mut self, max: Duration) -> Self {
        self.max_retry_after = max;
        self
    }

    /// An overall bound on one read, attempts and waits included. A wait that would not fit is
    /// not started: the last response is returned. If no response arrived in time, the read is
    /// [`Error::DeadlineExceeded`].
    #[must_use]
    pub fn read_deadline(mut self, deadline: Duration) -> Self {
        self.read_deadline = Some(deadline);
        self
    }

    async fn read<T: Send + 'static>(&self, prepared: Prepared<T>) -> Result<T, Error> {
        let mut bounds = concord_retry::Policy::new(self.read_attempts);
        bounds.timeout = self.read_deadline;
        let max = self.max_retry_after;
        let epoch = Instant::now();
        let run = Retry::new(bounds, Replay::Permitted)
            .receive(
                prepared,
                ReplayMode::Required,
                |_, _| {}, // the bearer credential is fixed for the client's lifetime
                |outcome| classify_read(outcome, max),
                move || epoch.elapsed(),
                tokio::time::sleep,
                |_, ceiling| ceiling, // no local backoff: the ceiling is zero
            )
            .await;
        match run.outcome {
            None => Err(Error::DeadlineExceeded),
            Some(Err(e)) => Err(Error::Concord(e)),
            Some(Ok(received)) if received.status().is_success() => Ok(received.decode().await?),
            Some(Ok(received)) => Err(Error::Http(Box::new(
                HttpError::read(received, run.report.attempts).await,
            ))),
        }
    }

    async fn write<T: Send + 'static>(
        &self,
        prepared: Prepared<T>,
        operation: &'static str,
    ) -> Result<T, Error> {
        let key = prepared
            .request()
            .headers()
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| IdempotencyKey::new(v).ok());
        let uncertain =
            |cause| Error::Uncertain(Box::new(UncertainWrite::new(operation, key.clone(), cause)));
        match prepared.receive().await {
            // No connection was established: the request never left this process.
            Err(e) if e.delivery() == Delivery::NotSent => Err(Error::Concord(e)),
            Err(e) => Err(uncertain(UncertainCause::NoResponse(e))),
            Ok(received) if received.status().is_server_error() => {
                let http = HttpError::read(received, 1).await;
                Err(uncertain(UncertainCause::ServerError(Box::new(http))))
            }
            Ok(received)
                if operation == "Zendesk.ticket.update"
                    && received.status() == StatusCode::CONFLICT =>
            {
                Err(Error::Collision(Box::new(
                    HttpError::read(received, 1).await,
                )))
            }
            Ok(received) if !received.status().is_success() => {
                Err(Error::Http(Box::new(HttpError::read(received, 1).await)))
            }
            Ok(received) => received
                .decode()
                .await
                .map_err(|e| uncertain(UncertainCause::UnreadableSuccess(e))),
        }
    }
}

/// A read is repeated only for a `429`, or a `503` with `Retry-After`, whose delay is parsed and
/// within `max`.
fn classify_read<T>(outcome: &Result<Received<T>, concord::Error>, max: Duration) -> Decision {
    let Ok(received) = outcome else {
        return Decision::Done; // no blanket retry of transport failures
    };
    let status = received.status();
    if status != StatusCode::TOO_MANY_REQUESTS && status != StatusCode::SERVICE_UNAVAILABLE {
        return Decision::Done;
    }
    let hint = concord_retry::http::retry_after(
        received
            .head()
            .headers()
            .get_all(RETRY_AFTER)
            .iter()
            .map(|v| v.as_bytes()),
        SystemTime::now(),
    );
    match hint {
        Ok(Some(delay)) if delay <= max => Decision::RetryAfter(Some(delay)),
        _ => Decision::Stop,
    }
}

impl<T: Send + 'static> SendWith<T> for SendPolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let operation = E::operation();
        let prepared = endpoint.prepare()?;
        if operation.method() == Some("GET") {
            self.read(prepared).await
        } else {
            self.write(prepared, operation.name()).await
        }
    }
}
