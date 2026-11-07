//! Strict readers of one response header field, for any [`HeaderMap`]: a response head
//! ([`ResponseHead::single`](crate::ResponseHead::single) delegates here), the headers kept in a
//! [`ResponseMeta`](crate::ResponseMeta) or an [`Error`](crate::Error), or a response obtained
//! natively. Provider header names and meanings (a rate-limit counter, a reset in seconds or as
//! an epoch) stay the SDK's.
//!
//! ```
//! use concord_core::field::{self, FieldError};
//! use concord_core::reqwest::header::{HeaderMap, HeaderValue};
//!
//! let mut headers = HeaderMap::new();
//! headers.insert("x-ratelimit-remaining", HeaderValue::from_static(" 42 "));
//! headers.append("x-request-id", HeaderValue::from_static("a"));
//! headers.append("x-request-id", HeaderValue::from_static("b"));
//! headers.insert("x-ratelimit-reset", HeaderValue::from_static("+5"));
//!
//! assert_eq!(field::unsigned(&headers, "x-ratelimit-remaining"), Ok(Some(42)));
//! assert_eq!(field::unsigned(&headers, "x-ratelimit-limit"), Ok(None)); // absent
//! assert_eq!(field::single(&headers, "x-request-id"), Err(FieldError::Repeated));
//! assert_eq!(field::unsigned(&headers, "x-ratelimit-reset"), Err(FieldError::Invalid)); // no sign
//! ```

use reqwest::header::{AsHeaderName, HeaderMap, HeaderValue};

pub use crate::codec::FieldError;

/// The one field line of `name`, if present: `Ok(None)` when absent, [`FieldError::Repeated`]
/// when the field has several lines (equal or not). For a provider header that must appear at
/// most once (a request id, a rate-limit counter).
///
/// # Errors
///
/// [`FieldError::Repeated`].
pub fn single(
    headers: &HeaderMap,
    name: impl AsHeaderName,
) -> Result<Option<&HeaderValue>, FieldError> {
    let mut values = headers.get_all(name).iter();
    match (values.next(), values.next()) {
        (None, _) => Ok(None),
        (Some(one), None) => Ok(Some(one)),
        (Some(_), Some(_)) => Err(FieldError::Repeated),
    }
}

/// The field `name` as a non-negative decimal integer (`X-RateLimit-Remaining: 42`): one field
/// line of ASCII digits, surrounding spaces and tabs ignored; no sign, no fraction, no exponent.
/// `Ok(None)` when absent. Rust's `str::parse::<u64>` would accept `+5`; this does not.
///
/// # Errors
///
/// [`FieldError::Repeated`], [`FieldError::Invalid`] (empty, not text, or not only digits) or
/// [`FieldError::Overflow`] (more than `u64::MAX`). The value is never part of the error.
pub fn unsigned(headers: &HeaderMap, name: impl AsHeaderName) -> Result<Option<u64>, FieldError> {
    let Some(value) = single(headers, name)? else {
        return Ok(None);
    };
    let digits = value.as_bytes().trim_ascii();
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return Err(FieldError::Invalid);
    }
    digits
        .iter()
        .try_fold(0u64, |n, d| {
            n.checked_mul(10)?.checked_add(u64::from(d - b'0'))
        })
        .map(Some)
        .ok_or(FieldError::Overflow)
}
