//! Strict `Retry-After` projection, independent of reqwest and Concord.
//!
//! Pass **all** field lines (for example `headers.get_all("retry-after").iter().map(|v|
//! v.as_bytes())`), not just the first. Only IMF-fixdate is accepted: obsolete dates' two-digit
//! year interpretation is intentionally avoided. One date comma is syntax, not a list separator.

use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// A rejected field; raw values are never stored or printed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryAfterError {
    /// Neither unsigned decimal seconds nor a valid IMF-fixdate.
    Invalid,
    /// Several field lines or a comma list; `Retry-After` is a singleton.
    Repeated,
    /// Decimal seconds or the platform's date representation overflowed.
    Overflow,
}

impl fmt::Display for RetryAfterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "invalid Retry-After",
            Self::Repeated => "repeated Retry-After",
            Self::Overflow => "Retry-After overflow",
        })
    }
}
impl std::error::Error for RetryAfterError {}

/// Parses all field lines using an injected wall-clock snapshot for HTTP-dates.
/// Missing is `Ok(None)`; a past date is `Ok(Some(Duration::ZERO))`. Surrounding HTTP OWS is
/// accepted; signs, fractions, non-ASCII data and obsolete date formats are rejected.
///
/// # Errors
/// Returns [`RetryAfterError`] for invalid, repeated or overflowing values. No error echoes input.
pub fn retry_after<'a>(
    values: impl IntoIterator<Item = &'a [u8]>,
    now: SystemTime,
) -> Result<Option<Duration>, RetryAfterError> {
    use RetryAfterError::{Invalid, Overflow, Repeated};
    let mut values = values.into_iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(Repeated);
    }
    let start = value
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(value.len());
    let value = &value[start..];
    let end = value
        .iter()
        .rposition(|b| !matches!(b, b' ' | b'\t'))
        .map_or(0, |i| i + 1);
    let value = &value[..end];
    // Only SP / HTAB are OWS; other ASCII whitespace is invalid even at the edges.
    let original = std::str::from_utf8(value).map_err(|_| Invalid)?;
    let commas = value.iter().filter(|&&b| b == b',').count();
    let weekday = value
        .get(..3)
        .and_then(|v| weekdays().iter().position(|d| d.as_bytes() == v));
    let date_comma = (weekday.is_some() && value.get(3) == Some(&b','))
        || [
            "Sunday,",
            "Monday,",
            "Tuesday,",
            "Wednesday,",
            "Thursday,",
            "Friday,",
            "Saturday,",
        ]
        .iter()
        .any(|day| original.starts_with(day));
    if commas > 1 || (commas == 1 && !date_comma) {
        return Err(Repeated);
    }
    if !value.is_empty() && value.iter().all(u8::is_ascii_digit) {
        let seconds = value
            .iter()
            .try_fold(0u64, |n, b| {
                n.checked_mul(10)?.checked_add(u64::from(b - b'0'))
            })
            .ok_or(Overflow)?;
        return Ok(Some(Duration::from_secs(seconds)));
    }
    let date = parse_date(original, weekday.ok_or(Invalid)?)?;
    Ok(Some(date.duration_since(now).unwrap_or_default()))
}

fn weekdays() -> [&'static str; 7] {
    ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
}

fn parse_date(value: &str, weekday: usize) -> Result<SystemTime, RetryAfterError> {
    use RetryAfterError::{Invalid, Overflow};
    let b = value.as_bytes();
    if b.len() != 29
        || &b[3..5] != b", "
        || b[7] != b' '
        || b[11] != b' '
        || b[16] != b' '
        || b[19] != b':'
        || b[22] != b':'
        || &b[25..] != b" GMT"
    {
        return Err(Invalid);
    }
    let number = |bytes: &[u8]| -> Result<i64, RetryAfterError> {
        if !bytes.iter().all(u8::is_ascii_digit) {
            return Err(Invalid);
        }
        Ok(bytes.iter().fold(0, |n, b| n * 10 + i64::from(b - b'0')))
    };
    let year = number(&b[12..16])?;
    let months = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = months
        .iter()
        .position(|m| m.as_bytes() == &b[8..11])
        .ok_or(Invalid)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let lengths = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let day = number(&b[5..7])?;
    let hour = number(&b[17..19])?;
    let minute = number(&b[20..22])?;
    let second = number(&b[23..25])?;
    if year == 0 || day < 1 || day > lengths[month] || hour > 23 || minute > 59 || second > 60 {
        return Err(Invalid);
    }
    let days_before_year = |year: i64| {
        let y = year - 1;
        365 * y + y / 4 - y / 100 + y / 400
    };
    let days = days_before_year(year) - days_before_year(1970)
        + lengths[..month].iter().sum::<i64>()
        + day
        - 1;
    if (days + 4).rem_euclid(7) != weekday as i64 {
        return Err(Invalid);
    }
    let seconds = days * 86400 + hour * 3600 + minute * 60 + second;
    let delta = Duration::from_secs(seconds.unsigned_abs());
    if seconds >= 0 {
        UNIX_EPOCH.checked_add(delta)
    } else {
        UNIX_EPOCH.checked_sub(delta)
    }
    .ok_or(Overflow)
}
