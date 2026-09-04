use std::time::{Duration, SystemTime};

use http::HeaderMap;
use http::header::RETRY_AFTER;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ServerDelaySource {
    RetryAfter,
}

/// One bounded, core-parsed response-head delay fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ServerDelayHint {
    delay: Duration,
    source: ServerDelaySource,
}

impl ServerDelayHint {
    pub(crate) const fn delay(self) -> Duration {
        self.delay
    }

    #[allow(
        dead_code,
        reason = "retained for bounded event and vendor-hint expansion"
    )]
    pub(crate) const fn source(self) -> ServerDelaySource {
        self.source
    }
}

pub(crate) fn parse_server_delay(headers: &HeaderMap, now: SystemTime) -> Option<ServerDelayHint> {
    let value = headers.get(RETRY_AFTER)?.to_str().ok()?.trim();
    let delay = if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        let seconds = value.bytes().try_fold(0_u64, |seconds, byte| {
            seconds.checked_mul(10)?.checked_add(u64::from(byte - b'0'))
        });
        Duration::from_secs(seconds.unwrap_or(u64::MAX))
    } else {
        let date = httpdate::parse_http_date(value).ok()?;
        date.duration_since(now).ok()?
    };
    Some(ServerDelayHint {
        delay,
        source: ServerDelaySource::RetryAfter,
    })
}

#[cfg(test)]
mod tests {
    use std::time::UNIX_EPOCH;

    use http::HeaderValue;

    use super::*;

    fn hint(value: &str, now: SystemTime) -> Option<Duration> {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_str(value).expect("header"));
        parse_server_delay(&headers, now).map(ServerDelayHint::delay)
    }

    #[test]
    fn parses_delta_and_date_once_into_a_finite_fact() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(hint("12", now), Some(Duration::from_secs(12)));
        assert_eq!(hint("0", now), Some(Duration::ZERO));
        assert_eq!(
            hint("999999999999999999999999", now),
            Some(Duration::from_secs(u64::MAX))
        );
        let future = httpdate::fmt_http_date(now + Duration::from_secs(30));
        assert_eq!(hint(&future, now), Some(Duration::from_secs(30)));
        let past = httpdate::fmt_http_date(now - Duration::from_secs(1));
        assert_eq!(hint(&past, now), None);
    }

    #[test]
    fn malformed_values_are_ignored() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        for value in ["", "-1", "+1", "1.5", "tomorrow", "12 seconds"] {
            assert_eq!(hint(value, now), None);
        }
    }
}
