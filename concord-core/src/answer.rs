//! A call's value together with the response's status and headers.

use crate::error::Fail;
use std::time::{Duration, SystemTime};

// Share the retry package's parser without adding a format or a dependency.
// PANIC-SAFETY: Both consumers compile the same private HTTP-date parser; no panic lint is relaxed.
#[allow(clippy::duplicate_mod)] // The parser is private to retry; both consumers use the same source.
#[path = "retry/http_date.rs"]
mod http_date;

/// A timer for one pending body piece, supplied by the call engine.
///
/// The future owns the whole-call deadline and a fresh idle deadline. Dropping it
/// cancels this idle wait. Ready body work is always polled before this timer.
#[doc(hidden)]
pub trait BodyClock: Send + Sync {
    /// Start watching one wait; the returned future must not borrow the clock.
    fn wait(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = BodyTimeout> + Send>>;

    /// Copy progressive evidence before the engine can cancel a known outcome.
    /// The engine clears this per-try evidence before starting another try.
    fn record(&self, evidence: KnownOutcome<'_>);
}

/// Which body timer expired and the configured duration to report.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub enum BodyTimeout {
    /// Silence while awaiting one piece.
    Idle(Duration),
    /// Silence with a length set by the description rather than the caller.
    IdleFromDescription(Duration),
    /// The deadline of the whole call.
    Total(Duration),
}

/// An outcome already decided at the head.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownKind {
    /// An unclaimed refusal status.
    Refused,
    /// An unclaimed success on a HEAD with an arrow.
    NoArm,
}

/// Bounded evidence to copy into the call engine's per-try storage.
#[doc(hidden)]
pub struct KnownOutcome<'a> {
    /// The outcome that must survive cancellation.
    pub kind: KnownKind,
    /// The response status.
    pub status: http::StatusCode,
    /// The response headers.
    pub headers: &'a Headers,
    /// At most 64 KiB of body bytes, owned by the collector.
    pub body: &'a [u8],
    /// Whether the collection ended, was cut, or is still in progress.
    pub kept: crate::error::Kept,
}

impl std::fmt::Debug for KnownOutcome<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KnownOutcome")
            .field("kind", &self.kind)
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("kept", &self.kept)
            .finish_non_exhaustive()
    }
}

/// The headers of a response.
#[derive(Clone, Default)]
pub struct Headers(http::HeaderMap);

impl std::fmt::Debug for Headers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.0.keys().map(http::HeaderName::as_str))
            .finish()
    }
}

impl Headers {
    /// Returns the value of a header, if it is present and is valid text.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(|v| v.to_str().ok())
    }

    /// Returns the value of a header that must be present.
    ///
    /// In an arm or in a pager, `headers.require("location")?` ends the call with a
    /// Mismatch when the header is missing: the response is not what was promised.
    pub fn require<P>(&self, name: &'static str) -> Result<&str, Fail<P>> {
        self.get(name)
            .ok_or_else(|| crate::mismatch(format!("header {name} is missing or is not text")))
    }

    /// Returns the delay of `Retry-After`, given in seconds or as a date. A date in the past yields zero.
    pub fn retry_after(&self) -> Option<Duration> {
        self.retry_after_at(SystemTime::now())
    }

    /// Read `Retry-After` against a supplied wall clock, for deterministic decisions.
    #[doc(hidden)]
    pub fn retry_after_at(&self, now: SystemTime) -> Option<Duration> {
        let text = self.get("retry-after")?.trim();
        if !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()) {
            Some(Duration::from_secs(text.parse::<u64>().unwrap_or(u64::MAX)))
        } else {
            http_date::parse(text, now).map(|date| date.duration_since(now).unwrap_or_default())
        }
    }

    /// Returns the headers as the `http` crate represents them.
    pub fn as_map(&self) -> &http::HeaderMap {
        &self.0
    }
}

impl From<http::HeaderMap> for Headers {
    fn from(map: http::HeaderMap) -> Headers {
        Headers(map)
    }
}

/// A response's value with its status and headers.
///
/// `send_full()` returns one, and a pager receives one.
/// Its `Debug` shows the status and the header names, with no header or body values.
#[derive(Clone)]
pub struct Answer<T> {
    status: http::StatusCode,
    headers: Headers,
    value: T,
    // The engine resolves a relative next-page URL against this response URL.
    pub(crate) response_url: Option<reqwest::Url>,
}

impl<T> std::fmt::Debug for Answer<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Answer")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

impl<T> Answer<T> {
    /// Creates an `Answer`; for generated code and for tests.
    pub fn new(status: http::StatusCode, headers: Headers, value: T) -> Answer<T> {
        Answer {
            status,
            headers,
            value,
            response_url: None,
        }
    }

    /// Generated code keeps the response URL for the engine; it has no public getter.
    #[doc(hidden)]
    pub fn with_response_url(mut self, url: reqwest::Url) -> Answer<T> {
        self.response_url = Some(url);
        self
    }

    /// Returns the status of the response.
    pub fn status(&self) -> http::StatusCode {
        self.status
    }

    /// Returns the headers of the response.
    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    /// Borrows the decoded body, while the `Answer` keeps its status and headers.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns the value, consuming the `Answer`.
    pub fn into_value(self) -> T {
        self.value
    }

    /// Splits the `Answer` into its status, headers and value.
    pub fn into_parts(self) -> (http::StatusCode, Headers, T) {
        (self.status, self.headers, self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers_debug_shows_names_and_no_values() {
        let mut map = http::HeaderMap::new();
        map.insert(
            "authorization",
            "Bearer secret-header-value".parse().unwrap(),
        );
        map.insert("x-request-id", "private-request-id".parse().unwrap());
        let debug = format!("{:?}", Headers::from(map));
        assert!(debug.contains("authorization") && debug.contains("x-request-id"));
        assert!(!debug.contains("secret-header-value") && !debug.contains("private-request-id"));
    }

    #[test]
    fn answer_debug_shows_status_and_names_without_requiring_debug_of_the_value() {
        struct PrivateValue;
        let mut map = http::HeaderMap::new();
        map.insert("x-private", "secret-header-value".parse().unwrap());
        let answer = Answer::new(http::StatusCode::CREATED, Headers::from(map), PrivateValue)
            .with_response_url("https://example.com/private-url".parse().unwrap());
        let debug = format!("{answer:?}");
        assert!(debug.contains("201") && debug.contains("x-private"));
        assert!(!debug.contains("secret-header-value") && !debug.contains("private-url"));
        assert!(!debug.contains("PrivateValue") && !debug.contains("value"));
        assert_eq!(answer.response_url.as_ref().unwrap().path(), "/private-url");
    }
}

#[cfg(test)]
mod c5_tests {
    use super::*;
    use crate::{__private::arm, error::Kind, transport::TryInfo};

    #[test]
    fn c5_8_require_is_case_insensitive_and_missing_or_nontext_is_a_mismatch() {
        let mut map = http::HeaderMap::new();
        map.insert("location", "/next".parse().unwrap());
        map.insert("x-binary", http::HeaderValue::from_bytes(b"\xff").unwrap());
        let headers = Headers::from(map);
        assert_eq!(headers.require::<()>("LOCATION").unwrap(), "/next");
        assert!(headers.get("x-binary").is_none());
        for name in ["missing", "x-binary"] {
            let error: crate::Error = arm(
                headers.require::<()>(name),
                http::StatusCode::CREATED,
                &headers,
                &TryInfo::new("tasks.create", 3),
            )
            .unwrap_err();
            assert_eq!(error.kind(), Kind::Mismatch);
            assert_eq!(error.status(), Some(http::StatusCode::CREATED));
            assert!(error.reason().unwrap().contains(name));
            assert_eq!(error.body(), None);
            assert_eq!(error.tries(), 3);
        }
    }

    #[test]
    fn answer_keeps_head_value_and_hidden_response_url() {
        let mut map = http::HeaderMap::new();
        map.append("x-item", "one".parse().unwrap());
        map.append("x-item", "two".parse().unwrap());
        let answer = Answer::new(http::StatusCode::CREATED, map.into(), vec![1, 2])
            .with_response_url("https://example.com/a".parse().unwrap());
        assert_eq!(answer.status(), http::StatusCode::CREATED);
        assert_eq!(
            answer.headers().as_map().get_all("X-Item").iter().count(),
            2
        );
        assert_eq!(answer.value(), &[1, 2]);
        assert_eq!(answer.clone().into_value(), vec![1, 2]);
        let (status, headers, value) = answer.into_parts();
        assert_eq!(status, http::StatusCode::CREATED);
        assert_eq!(headers.get("x-item"), Some("one"));
        assert_eq!(value, vec![1, 2]);
    }

    #[test]
    fn c4_3_headers_read_retry_after_against_the_supplied_clock() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(784111777);
        for (text, expected) in [
            ("12", Some(12)),
            (" 12 ", Some(12)),
            ("Sun, 06 Nov 1994 08:49:49 GMT", Some(12)),
            ("Sunday, 06-Nov-94 08:49:49 GMT", Some(12)),
            ("Sun Nov  6 08:49:49 1994", Some(12)),
            ("Sun, 06 Nov 1994 08:49:37 GMT", Some(0)),
            ("bad", None),
            ("-1", None),
            ("1.5", None),
            ("", None),
        ] {
            let mut map = http::HeaderMap::new();
            map.insert("retry-after", text.parse().unwrap());
            assert_eq!(
                Headers::from(map).retry_after_at(now),
                expected.map(Duration::from_secs),
                "{text}"
            );
        }
        assert_eq!(Headers::default().retry_after_at(now), None);
    }
}
