//! The one core-parsed standard server-delay fact per response head (§13.6).

use std::time::{Duration, SystemTime};

use http::HeaderMap;
use http::header::RETRY_AFTER;
use tokio::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServerDelaySource {
    RetryAfter,
}

/// One bounded, core-parsed response-head delay fact.
///
/// It is parsed exactly once when a head arrives, from the wall time captured
/// once for that head, and anchored to the monotonic instant of arrival. Every
/// consumer (observer events, limiter feedback, status-body diagnostics and
/// the retry gate) derives from this value; nobody reparses the header or
/// re-reads wall time. Diagnostics and limiter cooldown storage clamp their
/// own copies; the retry gate uses the untruncated minimum.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ServerDelayHint {
    delay: Duration,
    overlarge: bool,
    observed_at: Instant,
    source: ServerDelaySource,
}

impl ServerDelayHint {
    /// The untruncated server minimum measured from head arrival.
    /// `Duration::MAX` for an overlarge hint.
    pub(crate) const fn delay(self) -> Duration {
        self.delay
    }

    /// Whether checked arithmetic overflowed while parsing or anchoring the
    /// hint. An overlarge hint forbids policy retry; it never wraps to a
    /// short delay.
    pub(crate) const fn forbids_retry(self) -> bool {
        self.overlarge
    }

    /// The part of the server minimum still ahead at `now`
    /// (`server_not_before_remaining`).
    pub(crate) fn remaining_at(self, now: Instant) -> Duration {
        self.delay
            .saturating_sub(now.saturating_duration_since(self.observed_at))
    }

    #[allow(
        dead_code,
        reason = "retained for bounded event and vendor-hint expansion"
    )]
    pub(crate) const fn source(self) -> ServerDelaySource {
        self.source
    }
}

/// Parses the standard `Retry-After` field once.
///
/// Supported: nonnegative delta seconds (zero is a valid zero minimum) and a
/// valid HTTP date not in the past. Malformed, negative or past values yield
/// no hint. Multiple field occurrences, or a combined value that is not one
/// syntactically valid form, are ignored as malformed rather than guessed.
/// Checked overflow yields an overlarge hint that forbids retry.
pub(crate) fn parse_server_delay(
    headers: &HeaderMap,
    now: SystemTime,
    observed_at: Instant,
) -> Option<ServerDelayHint> {
    let mut values = headers.get_all(RETRY_AFTER).iter();
    let value = values.next()?;
    if values.next().is_some() {
        return None;
    }
    let value = value.to_str().ok()?.trim();
    let (delay, mut overlarge) =
        if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
            let seconds = value.bytes().try_fold(0_u64, |seconds, byte| {
                seconds.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
            });
            match seconds {
                Some(seconds) => (Duration::from_secs(seconds), false),
                None => (Duration::MAX, true),
            }
        } else {
            let date = httpdate::parse_http_date(value).ok()?;
            (date.duration_since(now).ok()?, false)
        };
    if observed_at.checked_add(delay).is_none() {
        overlarge = true;
    }
    Some(ServerDelayHint {
        delay: if overlarge { Duration::MAX } else { delay },
        overlarge,
        observed_at,
        source: ServerDelaySource::RetryAfter,
    })
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use http::HeaderValue;

    use super::*;

    fn parsed(values: &[&str], now: SystemTime) -> Option<ServerDelayHint> {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(RETRY_AFTER, HeaderValue::from_str(value).expect("header"));
        }
        parse_server_delay(&headers, now, Instant::now())
    }

    fn hint(value: &str, now: SystemTime) -> Option<Duration> {
        parsed(&[value], now).map(ServerDelayHint::delay)
    }

    #[tokio::test]
    async fn v3_e20_parses_delta_and_date_once_into_a_finite_fact() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(hint("12", now), Some(Duration::from_secs(12)));
        assert_eq!(hint(" 7 ", now), Some(Duration::from_secs(7)));
        assert_eq!(hint("0", now), Some(Duration::ZERO));
        let future = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(hint(&future, now), Some(Duration::from_secs(30)));
        let current = httpdate::fmt_http_date(now);
        assert_eq!(hint(&current, now), Some(Duration::ZERO));
        let past = httpdate::fmt_http_date(now - Duration::from_secs(1));
        assert_eq!(hint(&past, now), None);
    }

    #[tokio::test]
    async fn v3_e20_overflow_is_an_overlarge_hint_that_forbids_retry() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let overflow = parsed(&["999999999999999999999999"], now).expect("overlarge hint");
        assert!(overflow.forbids_retry());
        assert_eq!(overflow.delay(), Duration::MAX);
        let unanchorable = parsed(&["18446744073709551615"], now).expect("huge hint");
        assert!(unanchorable.forbids_retry());
        let ordinary = parsed(&["3"], now).expect("ordinary hint");
        assert!(!ordinary.forbids_retry());
    }

    #[tokio::test]
    async fn v3_e20_malformed_negative_duplicate_and_ambiguous_values_are_ignored() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for value in ["", "-1", "+1", "1.5", "tomorrow", "12 seconds", "1, 2"] {
            assert_eq!(hint(value, now), None, "{value:?} must be ignored");
        }
        let date = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(hint(&format!("{date}, 5"), now), None);
        assert!(parsed(&["5", "5"], now).is_none());
        assert!(parsed(&["5", &date], now).is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn v3_e20_remaining_minimum_is_measured_from_head_arrival() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let arrived = Instant::now();
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("5"));
        let hint = parse_server_delay(&headers, now, arrived).expect("hint");
        assert_eq!(hint.remaining_at(arrived), Duration::from_secs(5));
        assert_eq!(
            hint.remaining_at(arrived + Duration::from_secs(2)),
            Duration::from_secs(3)
        );
        assert_eq!(
            hint.remaining_at(arrived + Duration::from_secs(9)),
            Duration::ZERO
        );
    }
}
