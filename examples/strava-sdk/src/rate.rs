//! Strava's paired, calendar-aligned overall and non-upload windows.
use concord::{field, reqwest::header::HeaderMap};
use concord_ratelimit::{Limiter, Observation};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A single quota window. Counts are never assumed to be provider defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// Server-provided request limit.
    pub limit: u32,
    /// Server-provided consumed requests (may exceed the limit).
    pub used: u32,
}
impl Window {
    /// Saturating remaining count.
    pub fn remaining(self) -> u32 {
        self.limit.saturating_sub(self.used)
    }
    /// Whether this window is exhausted.
    pub fn exhausted(self) -> bool {
        self.used >= self.limit
    }
}
/// Short and daily windows of one quota class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quota {
    /// Natural 15-minute interval.
    pub short: Window,
    /// UTC calendar day.
    pub daily: Window,
}
/// Observations preserve missing and malformed pairs independently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RateLimits {
    /// Overall quota, charged by uploads and reads.
    pub overall: Result<Option<Quota>, concord::FieldError>,
    /// Additional non-upload quota, charged by activity reads and status polls.
    pub read: Result<Option<Quota>, concord::FieldError>,
}
impl RateLimits {
    /// Parse all four headers, rejecting repeated, signed, partial or overflowing pairs.
    pub fn parse(headers: &HeaderMap) -> Self {
        Self {
            overall: quota(headers, "x-ratelimit-limit", "x-ratelimit-usage"),
            read: quota(headers, "x-readratelimit-limit", "x-readratelimit-usage"),
        }
    }
    pub(crate) fn observe(
        &self,
        limiter: &Limiter<u8>,
        status: u16,
        read: bool,
        headers: &HeaderMap,
    ) -> Result<(), crate::Error> {
        let now = SystemTime::now();
        let wall = now
            .duration_since(UNIX_EPOCH)
            .map_err(|_| crate::Error::Invalid("clock before Unix epoch"))?;
        let mut identified = false;
        for (base, quota) in [(0, &self.overall), (2, &self.read)] {
            if base == 2 && !read {
                continue;
            }
            if let Ok(Some(quota)) = quota {
                for (key, window, period) in
                    [(base, quota.short, 900), (base + 1, quota.daily, 86400)]
                {
                    identified |= window.exhausted();
                    // Full fractional difference avoids reopening early within the final second.
                    let delay = Duration::from_secs(period - wall.as_secs() % period)
                        .saturating_sub(Duration::from_nanos(u64::from(wall.subsec_nanos())));
                    let reset_at = limiter.after(delay).map_err(crate::Error::Admission)?;
                    limiter
                        .observe(
                            key,
                            Observation::Window {
                                remaining: window.remaining(),
                                reset_at,
                            },
                        )
                        .map_err(crate::Error::Admission)?;
                }
            }
        }
        if status == 429 {
            let hint = concord_retry::http::retry_after(
                headers.get_all("retry-after").iter().map(|v| v.as_bytes()),
                now,
            )
            .ok()
            .flatten();
            if let Some(delay) = hint {
                limiter.observe_global(limiter.after(delay).map_err(crate::Error::Admission)?);
            } else if !identified {
                // A 429 with no usable explanation cannot justify a 15-minute-only cooldown.
                let delay = Duration::from_secs(86400 - wall.as_secs() % 86400);
                limiter.observe_global(limiter.after(delay).map_err(crate::Error::Admission)?);
            }
        }
        Ok(())
    }
}
fn pair(headers: &HeaderMap, name: &str) -> Result<Option<[u32; 2]>, concord::FieldError> {
    use concord::FieldError::{Invalid, Overflow};
    let Some(value) = field::single(headers, name)? else {
        return Ok(None);
    };
    let text = value.to_str().map_err(|_| Invalid)?;
    let mut parts = text.split(',');
    let mut out = [0; 2];
    for number in &mut out {
        let part = parts.next().ok_or(Invalid)?.trim_matches([' ', '\t']);
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Invalid);
        }
        *number = part.parse().map_err(|_| Overflow)?;
    }
    if parts.next().is_some() {
        return Err(Invalid);
    }
    Ok(Some(out))
}
fn quota(
    headers: &HeaderMap,
    limit: &str,
    usage: &str,
) -> Result<Option<Quota>, concord::FieldError> {
    match (pair(headers, limit)?, pair(headers, usage)?) {
        (None, None) => Ok(None),
        (Some(l), Some(u)) => Ok(Some(Quota {
            short: Window {
                limit: l[0],
                used: u[0],
            },
            daily: Window {
                limit: l[1],
                used: u[1],
            },
        })),
        _ => Err(concord::FieldError::Invalid),
    }
}
