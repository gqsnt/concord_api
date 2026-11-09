use crate::{
    ERROR_BODY_LIMIT, Error, HttpClass, Problem, ProviderError, RateLimits, Response,
    SentryOperation,
};
use concord::{Endpoint, Json, SendOperation};
use concord_ratelimit::{Budget, Lease, Limiter, Mode, Observation, TokioClock};
use concord_retry::{Decision, Policy, Replay, Retry, StopReason, concord::ReplayMode};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::{Duration, Instant, SystemTime},
};

/// Local bounds, not claims about Sentry's provider quotas.
#[derive(Clone, Copy, Debug)]
pub struct Settings {
    /// Attempts including the first; default three.
    pub max_attempts: NonZeroUsize,
    /// Total admission, dispatch and backoff deadline; must be positive.
    pub timeout: Duration,
    /// First exponential backoff and minimum concurrency recheck delay (at least 50ms).
    pub initial_delay: Duration,
    /// Maximum local backoff; server minimums can exceed it.
    pub max_delay: Duration,
    /// Local requests per fixed window, per endpoint across organizations.
    pub requests: NonZeroU32,
    /// Positive local frequency window.
    pub window: Duration,
    /// Local simultaneous requests per endpoint, held through body consumption.
    pub concurrency: NonZeroUsize,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            max_attempts: NonZeroUsize::new(3).unwrap(),
            timeout: Duration::from_secs(10),
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(2),
            requests: NonZeroU32::new(5).unwrap(),
            window: Duration::from_secs(1),
            concurrency: NonZeroUsize::new(1).unwrap(),
        }
    }
}

/// A cheap clone sharing five endpoint buckets (four reads, the status update) for one
/// provider caller identity.
/// Reuse across tokens for the same caller; construct separately for different callers.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct SendPolicy {
    limiter: Limiter<SentryOperation>,
    settings: Settings,
}

impl Default for SendPolicy {
    fn default() -> Self {
        Self::new(Settings::default()).expect("valid default settings")
    }
}
impl SendPolicy {
    /// Configure admission. Zero windows/deadlines are rejected before any request.
    pub fn new(settings: Settings) -> Result<Self, Error> {
        if settings.timeout.is_zero() || settings.window.is_zero() {
            return Err(Error::Configuration("window and timeout must be positive"));
        }
        let limiter = Limiter::new(
            NonZeroUsize::new(SentryOperation::ALL.len()).unwrap(),
            TokioClock::default(),
        );
        for &key in SentryOperation::ALL {
            limiter.set_budget(
                key,
                Budget {
                    requests: settings.requests,
                    window: settings.window,
                },
            )?;
            limiter.set_concurrency(key, settings.concurrency)?;
        }
        Ok(Self { limiter, settings })
    }
}

/// Every Sentry rule the policy keys on the operation besides its admission bucket (one per
/// operation).
struct Class {
    /// A write may have been applied when it got no answer or a 5xx: only its documented
    /// rate-limit rejection (429) is repeated.
    write: bool,
    /// A cursor listing: its `Link` header gives `Response::next_cursor`.
    paged: bool,
}
impl Class {
    /// No wildcard arm: an endpoint added to the declaration without a class here does not
    /// compile (E0004, naming its `SentryOperation` variant).
    fn of(operation: SentryOperation) -> Self {
        use SentryOperation as Op;
        match operation {
            Op::OrganizationIssues | Op::OrganizationIssueEvents => Self {
                write: false,
                paged: true,
            },
            Op::OrganizationIssueGet | Op::OrganizationIssueEvent => Self {
                write: false,
                paged: false,
            },
            Op::OrganizationIssueUpdateStatus => Self {
                write: true,
                paged: false,
            },
        }
    }
}
struct Attempt {
    _lease: Lease<SentryOperation>,
    rates: RateLimits,
    scheduling_error: Option<&'static str>,
}

impl<T: Send> SendOperation<T, SentryOperation> for SendPolicy {
    type Output = Result<Response<T>, Error>;
    async fn send<E: Endpoint<T> + Send>(
        &self,
        operation: SentryOperation,
        endpoint: E,
    ) -> Self::Output {
        let key = operation;
        let Class { write, paged } = Class::of(operation);
        let prepared = endpoint.prepare()?;
        let current = prepared.request().url().clone();
        let settings = self.settings;
        let fallback = settings.initial_delay.max(Duration::from_millis(50));
        let epoch = Instant::now();
        let run = Retry::new(
            Policy {
                max_attempts: settings.max_attempts,
                timeout: Some(settings.timeout),
                initial_delay: settings.initial_delay,
                max_delay: settings.max_delay,
            },
            Replay::Permitted,
        )
        .receive_with(
            prepared,
            ReplayMode::Required,
            |request, _| {
                let limiter = self.limiter.clone();
                async move {
                    // Every retry repeats BOTH admissions, after backoff, in this order.
                    let lease = limiter.lease(key, Mode::Wait).await?;
                    limiter.acquire(&[key], Mode::Wait).await?;
                    Ok::<_, Error>((
                        request,
                        Attempt {
                            _lease: lease,
                            rates: RateLimits::default(),
                            scheduling_error: None,
                        },
                    ))
                }
            },
            |outcome, context| {
                let received = match outcome {
                    Ok(received) => received,
                    Err(_) if write => return Decision::Done, // uncertain, reported below
                    Err(_) => return Decision::RetryAfter(None), // reads are safe to repeat
                };
                let scheduling = (|| {
                    let headers = received.head().headers();
                    context.rates = RateLimits::parse(headers)?;
                    context.rates.observe(
                        &self.limiter,
                        key,
                        settings.concurrency.get(),
                        fallback,
                    )?;
                    let hint = concord_retry::http::retry_after(
                        headers.get_all("retry-after").iter().map(|v| v.as_bytes()),
                        SystemTime::now(),
                    )
                    .map_err(|_| "invalid or repeated Retry-After")?;
                    let retryable = match received.status().as_u16() {
                        429 => true,
                        502..=504 => !write,
                        _ => false,
                    };
                    if retryable {
                        let delay = hint.unwrap_or_default().max(fallback);
                        let until = self
                            .limiter
                            .after(delay)
                            .map_err(|_| "retry delay overflow")?;
                        self.limiter
                            .observe(key, Observation::Cooldown(until))
                            .map_err(|_| "retry observation refused")?;
                        Ok(Decision::RetryAfter(Some(delay)))
                    } else {
                        Ok(Decision::Done)
                    }
                })();
                match scheduling {
                    Ok(decision) => decision,
                    Err(reason) => {
                        context.scheduling_error = Some(reason);
                        Decision::Done
                    }
                }
            },
            || epoch.elapsed(),
            tokio::time::sleep,
            |_, ceiling| ceiling,
        )
        .await;
        if let Some(error) = run.refusal {
            return Err(error);
        }
        // Writes repeat only after a 429, so a headless dispatch is always the last one.
        if write && run.report.no_response {
            let completed = run.report.completed == run.report.retry.attempts;
            return Err(Error::Uncertain {
                transport: match run.outcome {
                    Some(Err(error)) if completed => Some(Box::new(error)),
                    _ => None,
                },
                report: Box::new(run.report),
            });
        }
        // A write's final non-429 answer is its result even when the deadline expired just after
        // it arrived (the run still reports `Deadline`): reporting a stop would hide an applied
        // change. Reads keep the conservative stop.
        let answered_write =
            write && matches!(&run.outcome, Some(Ok(r)) if r.status().as_u16() != 429);
        if !answered_write
            && matches!(
                run.report.retry.stop,
                StopReason::Deadline | StopReason::NotReplayable
            )
        {
            return Err(Error::RetryStopped {
                meta: run.outcome.as_ref().and_then(|o| match o {
                    Ok(r) => Some(Box::new(r.meta())),
                    Err(e) => e.head_meta().map(Box::new),
                }),
                report: Box::new(run.report),
            });
        }
        let received = run.outcome.ok_or_else(|| Error::RetryStopped {
            report: Box::new(run.report.clone()),
            meta: None,
        })??;
        let context = run
            .context
            .ok_or(Error::Configuration("missing final admission lease"))?;
        // `context` owns the lease until final decode or bounded error-document consumption ends.
        let result = if !received.status().is_success() {
            let class = HttpClass::of(received.status());
            let (meta, document) = received.document::<Json, Problem>(ERROR_BODY_LIMIT).await;
            Err(Error::Provider(Box::new(ProviderError {
                class,
                meta,
                document,
                scheduling_error: context.scheduling_error,
                retry: run.report,
            })))
        } else if let Some(reason) = context.scheduling_error {
            Err(Error::RateMetadata {
                reason,
                meta: Box::new(received.meta()),
            })
        } else {
            let next_cursor = if paged {
                crate::pagination::next_cursor(received.head().headers(), &current).map_err(
                    |reason| Error::Pagination {
                        reason,
                        meta: Box::new(received.meta()),
                    },
                )?
            } else {
                None
            };
            let (value, meta) = received.decode_with_meta().await?;
            Ok(Response {
                value,
                meta,
                next_cursor,
                rates: context.rates,
                retry: run.report,
            })
        };
        drop(context);
        result
    }
}
