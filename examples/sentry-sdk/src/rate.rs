use crate::SentryOperation;
use concord::{field, reqwest::header::HeaderMap};
use concord_ratelimit::{Limiter, Observation};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Sentry's independent frequency window and simultaneous-request observations.
/// Missing differs from zero; invalid or repeated fields refuse scheduling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RateLimits {
    /// Requests per provider window.
    pub limit: Option<u32>,
    /// Requests remaining in the frequency window.
    pub remaining: Option<u32>,
    /// UTC seconds since epoch for the next frequency window.
    pub reset: Option<u64>,
    /// Maximum simultaneous provider requests.
    pub concurrent_limit: Option<u32>,
    /// Snapshot of available provider concurrency, including other processes.
    pub concurrent_remaining: Option<u32>,
}
impl RateLimits {
    pub(crate) fn parse(headers: &HeaderMap) -> Result<Self, &'static str> {
        let small = |name| -> Result<Option<u32>, &'static str> {
            field::unsigned(headers, name)
                .map_err(|_| "invalid or repeated quota field")?
                .map(|n| u32::try_from(n).map_err(|_| "quota field overflow"))
                .transpose()
        };
        let limits = Self {
            limit: small("x-sentry-rate-limit-limit")?,
            remaining: small("x-sentry-rate-limit-remaining")?,
            reset: field::unsigned(headers, "x-sentry-rate-limit-reset")
                .map_err(|_| "invalid or repeated reset field")?,
            concurrent_limit: small("x-sentry-rate-limit-concurrentlimit")?,
            concurrent_remaining: small("x-sentry-rate-limit-concurrentremaining")?,
        };
        if limits.limit == Some(0) || limits.concurrent_limit == Some(0) {
            return Err("zero provider limit");
        }
        if limits
            .remaining
            .zip(limits.limit)
            .is_some_and(|(a, b)| a > b)
            || limits
                .concurrent_remaining
                .zip(limits.concurrent_limit)
                .is_some_and(|(a, b)| a > b)
        {
            return Err("remaining exceeds provider limit");
        }
        if limits.remaining.is_some() != limits.reset.is_some() {
            return Err("frequency remaining and reset must appear together");
        }
        Ok(limits)
    }

    pub(crate) fn observe(
        self,
        limiter: &Limiter<SentryOperation>,
        key: SentryOperation,
        local_concurrency: usize,
        fallback: Duration,
    ) -> Result<(), &'static str> {
        if let Some(limit) = self.concurrent_limit {
            let limit = local_concurrency.min(limit as usize);
            limiter
                .set_concurrency(
                    key,
                    std::num::NonZeroUsize::new(limit).ok_or("zero concurrency")?,
                )
                .map_err(|_| "concurrency observation refused")?;
        }
        if let (Some(remaining), Some(reset)) = (self.remaining, self.reset) {
            let until = UNIX_EPOCH
                .checked_add(Duration::from_secs(reset))
                .ok_or("reset time overflow")?;
            let delay = until.duration_since(SystemTime::now()).unwrap_or_default();
            let reset_at = limiter.after(delay).map_err(|_| "reset time overflow")?;
            limiter
                .observe(
                    key,
                    Observation::Window {
                        remaining,
                        reset_at,
                    },
                )
                .map_err(|_| "frequency observation refused")?;
        }
        // This snapshot cannot identify the lifetime of another process's requests.
        // Recheck after a conservative delay; never turn it into a frequency token count.
        if self.concurrent_remaining == Some(0) {
            let until = limiter
                .after(fallback)
                .map_err(|_| "concurrency delay overflow")?;
            limiter
                .observe(key, Observation::Cooldown(until))
                .map_err(|_| "concurrency observation refused")?;
        }
        Ok(())
    }
}
