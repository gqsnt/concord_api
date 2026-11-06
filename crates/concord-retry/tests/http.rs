//! Strict singleton Retry-After parser cases.

use concord_retry::http::{RetryAfterError as E, retry_after};
use std::time::{Duration, UNIX_EPOCH};

fn parse(value: &[u8]) -> Result<Option<Duration>, E> {
    retry_after([value], UNIX_EPOCH + Duration::from_secs(784111777))
}

#[test]
fn missing_is_distinct_from_zero() {
    assert_eq!(
        retry_after(std::iter::empty::<&[u8]>(), UNIX_EPOCH),
        Ok(None)
    );
    assert_eq!(parse(b"0"), Ok(Some(Duration::ZERO)));
}

#[test]
fn unsigned_delay_seconds_and_http_ows() {
    for input in [b"1".as_slice(), b"0001", b" \t1\t "] {
        assert_eq!(parse(input), Ok(Some(Duration::from_secs(1))));
    }
    assert_eq!(
        parse(u64::MAX.to_string().as_bytes()),
        Ok(Some(Duration::from_secs(u64::MAX)))
    );
}

#[test]
fn date_uses_injected_wall_time_and_past_is_zero() {
    let date = b"Sun, 06 Nov 1994 08:49:37 GMT".as_slice();
    assert_eq!(
        retry_after([date], UNIX_EPOCH + Duration::from_secs(784111776)),
        Ok(Some(Duration::from_secs(1)))
    );
    assert_eq!(parse(date), Ok(Some(Duration::ZERO)));
    assert_eq!(
        retry_after([date], UNIX_EPOCH + Duration::from_secs(784111778)),
        Ok(Some(Duration::ZERO))
    );
    // Valid pre-epoch dates are past dates too, rather than parse errors.
    assert_eq!(
        parse(b"Wed, 31 Dec 1969 23:59:59 GMT"),
        Ok(Some(Duration::ZERO))
    );
    // Fractional wall clock is retained, rather than rounding a minimum down.
    assert_eq!(
        retry_after([date], UNIX_EPOCH + Duration::from_millis(784111776500)),
        Ok(Some(Duration::from_millis(500)))
    );
}

#[test]
fn malformed_and_obsolete_dates_are_invalid() {
    for input in [
        "",
        "-1",
        "+1",
        "1.0",
        "1 0",
        "1\n",
        "\r1",
        "1\x0b",
        "never-secret",
        "Sun, 06 Nov 1994 08:49:37 UTC",
        "Sun, 06 Nov 1994 08:49:37 gmt",
        "Mon, 06 Nov 1994 08:49:37 GMT",
        "Sun, 31 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 24:00:00 GMT",
        "Sun, 06 Nov 1994 08:60:00 GMT",
        "Sun, 06 Nov 1994 08:49:61 GMT",
        "Thu, 29 Feb 1900 00:00:00 GMT",
        "Sunday, 06-Nov-94 08:49:37 GMT",
        "Sun Nov  6 08:49:37 1994",
        "Sun, 06 Nov 1994 08:49:37 GMT junk",
    ] {
        assert_eq!(parse(input.as_bytes()), Err(E::Invalid), "{input:?}");
    }
    assert_eq!(parse(&[255]), Err(E::Invalid));
}

#[test]
fn repeated_field_lines_and_comma_lists_are_rejected_but_date_comma_is_allowed() {
    for lines in [
        vec![b"1".as_slice(), b"2"],
        vec![b"".as_slice(), b""],
        vec![b"Sun, 06 Nov 1994 08:49:37 GMT".as_slice(), b"1"],
    ] {
        assert_eq!(retry_after(lines, UNIX_EPOCH), Err(E::Repeated));
    }
    for input in [
        "1, 2",
        "1,",
        ",1",
        "Sun, 06 Nov 1994 08:49:37 GMT, 1",
        "1, Sun, 06 Nov 1994 08:49:37 GMT",
        "Sun, 06 Nov 1994 08:49:37 GMT, Sun, 06 Nov 1994 08:49:37 GMT",
    ] {
        assert_eq!(parse(input.as_bytes()), Err(E::Repeated));
    }
    assert_eq!(
        parse(b"Sun, 06 Nov 1994 08:49:37 GMT"),
        Ok(Some(Duration::ZERO))
    );
}

#[test]
fn decimal_overflow_does_not_echo_input() {
    for input in ["18446744073709551616".to_owned(), "9".repeat(500)] {
        assert_eq!(parse(input.as_bytes()), Err(E::Overflow));
    }
    for e in [E::Invalid, E::Repeated, E::Overflow] {
        assert!(!format!("{e} {e:?}").contains("never-secret"));
        assert!(std::error::Error::source(&e).is_none());
    }
}

#[test]
fn calendar_checks_leap_year_and_weekday() {
    assert_eq!(
        parse(b"Tue, 29 Feb 2000 00:00:00 GMT"),
        Ok(Some(Duration::from_secs(167670623)))
    );
    assert_eq!(parse(b"Mon, 29 Feb 2000 00:00:00 GMT"), Err(E::Invalid));
    assert!(retry_after([b"Fri, 31 Dec 9999 23:59:59 GMT".as_slice()], UNIX_EPOCH).is_ok());
}
