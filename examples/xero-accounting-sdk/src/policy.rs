//! One generated-send policy owns authentication, admission, repetition and bounded errors.
use crate::{Class, Error, ErrorKind, Problem, ProviderFailure, auth::Auth};
use concord::{
    Endpoint, Received, SendWith,
    reqwest::{Method, StatusCode, header::AUTHORIZATION},
};
use concord_ratelimit::{Limiter, Mode, Observation, TokioClock};
use concord_retry::{Decision, Policy as RetryPolicy, Replay, Retry};
use std::{
    num::NonZeroUsize,
    time::{Duration, Instant, SystemTime},
};

/// Finite success document bound (1 MiB).
pub const BODY_LIMIT: usize = 1024 * 1024;
/// HTTP error document bound (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

#[derive(Clone)]
pub(crate) struct Policy {
    pub auth: Auth,
    limiter: Limiter<()>,
}
impl Policy {
    pub fn new(auth: Auth) -> Self {
        let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), TokioClock::default());
        limiter
            .set_concurrency((), NonZeroUsize::new(5).unwrap())
            .expect("one key fits");
        Self { auth, limiter }
    }
}
impl<T: Send> SendWith<T> for Policy {
    type Output = Result<T, Error>;
    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let (http, request, decoder) = endpoint.prepare()?.into_parts();
        // The generated declaration is private; this extra check binds policy to header context.
        if request
            .headers()
            .get("xero-tenant-id")
            .and_then(|h| h.to_str().ok())
            != Some(self.auth.store.tenant().as_str())
        {
            return Err(Error::new(ErrorKind::TenantMismatch));
        }
        // Replay and read-scope eligibility require an actual GET; a declared read fact grants neither.
        let read = request.method() == Method::GET;
        self.auth.require(!read)?;
        let mut retry = RetryPolicy::new(NonZeroUsize::new(if read { 2 } else { 1 }).unwrap());
        retry.timeout = Some(Duration::from_secs(10));
        retry.initial_delay = Duration::from_millis(25);
        retry.max_delay = Duration::from_millis(25);
        let start = Instant::now();
        let run = Retry::new(
            retry,
            if read {
                Replay::Permitted
            } else {
                Replay::Never
            },
        )
        .run(
            |attempt| {
                // Declared requests have finite, cloneable JSON bodies; refresh is a separate service.
                let mut request = request.try_clone().expect("finite declaration request");
                let auth = self.auth.clone();
                let limiter = self.limiter.clone();
                let decoder = &decoder;
                let http = &http;
                async move {
                    let used = auth.token().await?;
                    let lease = limiter.lease((), Mode::Refuse).await?;
                    limiter.acquire(&[()], Mode::Refuse).await?;
                    request
                        .headers_mut()
                        .insert(AUTHORIZATION, used.header().clone());
                    let received = decoder.execute(http, request).await?;
                    if received.status() == StatusCode::UNAUTHORIZED {
                        auth.invalidate(&used);
                    }
                    if received.status() == StatusCode::TOO_MANY_REQUESTS {
                        // A missing/invalid hint is surfaced; no invented reset time or early retry.
                        if let Ok(Some(delay)) = retry_after(&received) {
                            limiter.observe((), Observation::Cooldown(limiter.after(delay)?))?;
                        }
                    }
                    // Release before backoff; retain only the final response lease through decode.
                    let retry_head = read
                        && attempt == 1
                        && received.status() == StatusCode::TOO_MANY_REQUESTS
                        && retry_after(&received).is_ok_and(|hint| {
                            hint.is_some_and(|delay| {
                                delay.max(Duration::from_millis(25))
                                    < Duration::from_secs(10).saturating_sub(start.elapsed())
                            })
                        });
                    let lease = if retry_head {
                        drop(lease);
                        None
                    } else {
                        Some(lease)
                    };
                    Ok::<_, Error>((received, lease))
                }
            },
            |outcome| match outcome {
                Ok((received, _)) if read && received.status() == StatusCode::TOO_MANY_REQUESTS => {
                    match retry_after(received) {
                        Ok(Some(delay)) => Decision::RetryAfter(Some(delay)),
                        _ => Decision::Done,
                    }
                }
                _ => Decision::Done,
            },
            || start.elapsed(),
            tokio::time::sleep,
            |_, ceiling| ceiling,
        )
        .await;
        let (received, _lease) = run
            .outcome
            .ok_or_else(|| Error::new(ErrorKind::Deadline))??;
        finish(received).await
    }
}
fn retry_after<T>(
    r: &Received<T>,
) -> Result<Option<Duration>, concord_retry::http::RetryAfterError> {
    concord_retry::http::retry_after(
        r.head()
            .headers()
            .get_all("retry-after")
            .iter()
            .map(|v| v.as_bytes()),
        SystemTime::now(),
    )
}
pub(crate) async fn finish<T>(received: Received<T>) -> Result<T, Error> {
    if !received.status().is_success() {
        let class = Class::status(received.status().as_u16());
        let retry_after = retry_after(&received);
        let (meta, document) = received
            .document::<concord::Json, Problem>(ERROR_BODY_LIMIT)
            .await;
        return Err(Error::new(ErrorKind::Provider(Box::new(ProviderFailure {
            class,
            meta,
            document: document.map_err(Box::new),
            retry_after,
        }))));
    }
    Ok(received.decode().await?)
}
