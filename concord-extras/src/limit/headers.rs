//! Bounded header shapes; unreadable input never changes prior knowledge.
use super::{Learn, Seen, Window};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use time::format_description::well_known::Rfc3339;
use time::{Date, Month, OffsetDateTime};

const MAX_HEADER_BYTES: usize = 4096;
const MAX_WINDOWS: usize = 8;

fn text<'a>(seen: &'a Seen<'_>, name: &str) -> Option<&'a str> {
    seen.header(name)
        .filter(|text| text.len() <= MAX_HEADER_BYTES)
}

fn number(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn pairs(text: &str) -> Option<Vec<(u64, u64)>> {
    let mut result = Vec::new();
    for pair in text.split(',') {
        if result.len() == MAX_WINDOWS {
            return None;
        }
        let (n, seconds) = pair.trim().split_once(':')?;
        let n = number(n)?;
        let seconds = number(seconds)?;
        if seconds == 0 || result.iter().any(|(_, old)| *old == seconds) {
            return None;
        }
        result.push((n, seconds));
    }
    result.sort_unstable_by_key(|(_, seconds)| *seconds);
    Some(result)
}

pub(super) fn counts(limits: String, counts: String) -> Box<Learn> {
    Box::new(move |seen| {
        let parse = || {
            let limits = pairs(text(seen, &limits)?)?;
            let counts = pairs(text(seen, &counts)?)?;
            if limits.len() != counts.len() {
                return None;
            }
            limits
                .into_iter()
                .zip(counts)
                .map(|((capacity, seconds), (used, count_seconds))| {
                    if capacity == 0 || seconds != count_seconds {
                        return None;
                    }
                    Some(
                        Window::rolling(capacity, Duration::from_secs(seconds))
                            .remaining(capacity.saturating_sub(used)),
                    )
                })
                .collect::<Option<Vec<_>>>()
        };
        parse().unwrap_or_default()
    })
}

pub(super) fn remaining(limit: String, remaining: String, reset: String) -> Box<Learn> {
    Box::new(move |seen| {
        let parse = || {
            let capacity = number(text(seen, &limit)?)?;
            let remaining = number(text(seen, &remaining)?)?;
            let reset = seen.until_unix(number(text(seen, &reset)?)?)?;
            if capacity == 0 || remaining > capacity || reset.is_zero() {
                return None;
            }
            Some(vec![Window::fixed(capacity, reset).remaining(remaining)])
        };
        parse().unwrap_or_default()
    })
}

pub(super) fn remaining_rfc3339(limit: String, remaining: String, reset: String) -> Box<Learn> {
    Box::new(move |seen| {
        let parse = || {
            let capacity = number(text(seen, &limit)?)?;
            let remaining = number(text(seen, &remaining)?)?;
            let at = rfc3339(text(seen, &reset)?)?;
            let until = until(at, seen.server_now())?;
            if capacity == 0 || remaining > capacity || until.is_zero() {
                return None;
            }
            let mut window = Window::fixed(capacity, until).remaining(remaining);
            // The reset as the server wrote it: a later head that names it again is the
            // same reset, even when its own `Date` makes it look further away.
            window.reset_at = Some(at);
            Some(vec![window])
        };
        parse().unwrap_or_default()
    })
}

/// More than fifty years of 365.2425 days ahead is not a reset, as for an HTTP date's year.
const FIFTY_YEARS: Duration = Duration::from_secs(1_577_847_600);

/// The time from `base` to a reset: zero when it is past, `None` beyond fifty years.
pub(super) fn until(reset: SystemTime, base: SystemTime) -> Option<Duration> {
    let until = reset.duration_since(base).unwrap_or_default();
    (until <= FIFTY_YEARS).then_some(until)
}

fn at_nanos(nanos: i128) -> Option<SystemTime> {
    let size = nanos.unsigned_abs();
    // PANIC-SAFETY: Division and remainder by a nonzero constant cannot overflow or panic.
    let seconds = u64::try_from(size / 1_000_000_000).ok()?;
    // PANIC-SAFETY: Division and remainder by a nonzero constant cannot overflow or panic.
    let fraction = u32::try_from(size % 1_000_000_000).ok()?;
    let offset = Duration::new(seconds, fraction);
    if nanos < 0 {
        UNIX_EPOCH.checked_sub(offset)
    } else {
        UNIX_EPOCH.checked_add(offset)
    }
}

/// An RFC 3339 date-time, read by `time` within the stricter rules of `Seen::until_rfc3339`.
pub(super) fn rfc3339(text: &str) -> Option<SystemTime> {
    if text.len() > MAX_HEADER_BYTES {
        return None;
    }
    let text = text.trim_matches([' ', '\t']);
    let bytes = text.as_bytes();
    // `time` takes any byte between the date and the time.
    if !matches!(bytes.get(10), Some(b'T' | b't' | b' ')) {
        return None;
    }
    // `time` reads a leap second as the nanosecond before it; it counts as the next second.
    let leap = bytes.get(16..19) == Some(b":60".as_slice());
    let parsed = if leap {
        let shifted = format!("{}59{}", text.get(..17)?, text.get(19..)?);
        OffsetDateTime::parse(&shifted, &Rfc3339).ok()?
    } else {
        OffsetDateTime::parse(text, &Rfc3339).ok()?
    };
    // `time` drops the digits past the nanosecond; a dropped digit that is not zero rounds up.
    let finer = bytes.get(19) == Some(&b'.')
        && bytes
            .get(20..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .skip(9)
            .any(|byte| *byte != b'0');
    let mut nanos = parsed.unix_timestamp_nanos();
    if leap {
        nanos = nanos.checked_add(1_000_000_000)?;
        // The second after a leap second starts the first day of a month, in UTC.
        let next = OffsetDateTime::from_unix_timestamp(
            i64::try_from(nanos.div_euclid(1_000_000_000)).ok()?,
        )
        .ok()?;
        if next.day() != 1 || next.hour() != 0 || next.minute() != 0 || next.second() != 0 {
            return None;
        }
    }
    if finer {
        nanos = nanos.checked_add(1)?;
    }
    at_nanos(nanos)
}

/// The `Date` of a response, in any of the three forms of an HTTP date, by the grammar that
/// the core uses for `Retry-After` (RFC 9110, section 5.6.7), on `time`'s calendar.
pub(super) fn http_date(text: &str, now: SystemTime) -> Option<SystemTime> {
    if text.len() > MAX_HEADER_BYTES {
        return None;
    }
    let text = text.trim_matches([' ', '\t']);
    let (year, month, day, weekday, clock) = if text.len() == 29 {
        // IMF-fixdate: `Sun, 06 Nov 1994 08:49:37 GMT`.
        let shape = text.get(3..5) == Some(", ")
            && [7, 11, 16]
                .iter()
                .all(|at| text.get(*at..=*at) == Some(" "))
            && text.get(25..) == Some(" GMT");
        if !shape {
            return None;
        }
        let year = i32::from(digits(text.get(12..16)?)?);
        (
            year,
            text.get(8..11)?,
            digits(text.get(5..7)?)?,
            text.get(..3)?,
            text.get(17..25)?,
        )
    } else if let Some((weekday, rest)) = text.split_once(", ") {
        // RFC 850: `Sunday, 06-Nov-94 08:49:37 GMT`, a two-digit year by the fifty-year rule.
        let shape = rest.len() == 22
            && rest.get(2..3) == Some("-")
            && rest.get(6..7) == Some("-")
            && rest.get(9..10) == Some(" ")
            && rest.get(18..) == Some(" GMT");
        if !shape || weekday.len() < 6 {
            return None;
        }
        let month = rest.get(3..6)?;
        let day = digits(rest.get(..2)?)?;
        let clock = rest.get(10..18)?;
        let year = century(digits(rest.get(7..9)?)?, month, day, clock, now)?;
        (year, month, day, weekday, clock)
    } else if text.len() == 24 {
        // asctime: `Sun Nov  6 08:49:37 1994`.
        let shape = [3, 7, 10, 19]
            .iter()
            .all(|at| text.get(*at..=*at) == Some(" "));
        if !shape {
            return None;
        }
        let day = text.get(8..10)?;
        let day = digits(day.strip_prefix(' ').unwrap_or(day))?;
        let year = i32::from(digits(text.get(20..)?)?);
        (
            year,
            text.get(4..7)?,
            day,
            text.get(..3)?,
            text.get(11..19)?,
        )
    } else {
        return None;
    };
    let date = calendar(year, month, day)?;
    let (hour, minute, second) = time_of_day(clock)?;
    // The written date names the weekday, also before a leap second rolls into the next day.
    let names = if weekday.len() == 3 {
        WEEKDAYS
    } else {
        LONG_WEEKDAYS
    };
    if names.get(usize::from(date.weekday().number_days_from_monday())) != Some(&weekday) {
        return None;
    }
    let midnight = i128::from(date.midnight().assume_utc().unix_timestamp());
    let of_day = i128::from(hour)
        .checked_mul(3600)?
        .checked_add(i128::from(minute).checked_mul(60)?)?
        .checked_add(i128::from(second))?;
    at_nanos(midnight.checked_add(of_day)?.checked_mul(1_000_000_000)?)
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const LONG_WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn digits(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// A date of the years 1 to 9999 that exists.
fn calendar(year: i32, month: &str, day: u16) -> Option<Date> {
    if !(1..=9999).contains(&year) {
        return None;
    }
    let month = MONTHS.iter().position(|name| *name == month)?;
    let month = Month::try_from(u8::try_from(month.checked_add(1)?).ok()?).ok()?;
    Date::from_calendar_date(year, month, u8::try_from(day).ok()?).ok()
}

/// `HH:MM:SS`, with a second of 60 only at 23:59:60, as the core reads it.
fn time_of_day(clock: &str) -> Option<(u16, u16, u16)> {
    if clock.len() != 8 || clock.get(2..3) != Some(":") || clock.get(5..6) != Some(":") {
        return None;
    }
    let (hour, minute) = (digits(clock.get(..2)?)?, digits(clock.get(3..5)?)?);
    let second = digits(clock.get(6..)?)?;
    let valid = hour < 24 && minute < 60 && (second < 60 || (hour, minute, second) == (23, 59, 60));
    valid.then_some((hour, minute, second))
}

/// The year of a two-digit RFC 850 year: the latest one that is not more than fifty years
/// after `now`, as the core reads it.
fn century(two: u16, month: &str, day: u16, clock: &str, now: SystemTime) -> Option<i32> {
    // Whole seconds, rounded down before 1970 as after it, as the core counts them.
    let seconds = match now.duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).ok()?,
        Err(before) => {
            let before = before.duration();
            let whole = i64::try_from(before.as_secs()).ok()?.checked_neg()?;
            whole.checked_sub(i64::from(before.subsec_nanos() != 0))?
        }
    };
    let now = OffsetDateTime::from_unix_timestamp(seconds).ok()?;
    // A clock outside the years 1 to 9999 chooses no century, as in the core.
    if !(1..=9999).contains(&now.year()) {
        return None;
    }
    let limit_year = now.year().checked_add(50)?;
    let month_number = MONTHS.iter().position(|name| *name == month)?;
    let written = (month_number, day, time_of_day(clock)?);
    let limit = (
        usize::from(u8::from(now.month())).checked_sub(1)?,
        u16::from(now.day()),
        (
            u16::from(now.hour()),
            u16::from(now.minute()),
            u16::from(now.second()),
        ),
    );
    let mut year = limit_year
        .checked_div(100)?
        .checked_mul(100)?
        .checked_add(i32::from(two))?;
    if (year, written) > (limit_year, limit) {
        year = year.checked_sub(100)?;
    }
    Some(year)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: u64, nanos: u32) -> Option<SystemTime> {
        Some(UNIX_EPOCH + Duration::new(seconds, nanos))
    }

    // 2024-08-20T18:37:24Z
    const T: u64 = 1_724_179_044;

    #[test]
    fn c11_5_rfc3339_accepts_each_listed_form() {
        for text in [
            "2024-08-20T18:37:24Z",
            "2024-08-20t18:37:24z",
            "2024-08-20 18:37:24Z",
            "2024-08-20T20:37:24+02:00",
            "2024-08-20T16:07:24-02:30",
            "2024-08-20T18:37:24-00:00",
            "2024-08-20T18:37:24.000Z",
            " \t2024-08-20T18:37:24Z\t ",
        ] {
            assert_eq!(rfc3339(text), at(T, 0), "{text}");
        }
        assert_eq!(rfc3339("2024-08-20T18:37:24.5Z"), at(T, 500_000_000));
        assert_eq!(rfc3339("2024-08-20T18:37:24.100435Z"), at(T, 100_435_000));
        assert_eq!(
            rfc3339("2024-08-20T18:37:24.123456789Z"),
            at(T, 123_456_789)
        );
    }

    #[test]
    fn c11_5_rfc3339_rounds_a_finer_fraction_up_never_down() {
        assert_eq!(
            rfc3339("2024-08-20T18:37:24.1234567891Z"),
            at(T, 123_456_790)
        );
        assert_eq!(rfc3339("2024-08-20T18:37:24.0000000001Z"), at(T, 1));
        assert_eq!(rfc3339("2024-08-20T18:37:24.9999999999Z"), at(T + 1, 0));
        // Zeros past the nanosecond lose nothing.
        assert_eq!(
            rfc3339("2024-08-20T18:37:24.1234567890000Z"),
            at(T, 123_456_789)
        );
    }

    #[test]
    fn c11_5_rfc3339_leap_second_counts_as_the_next_second_only_at_a_month_end() {
        // 2017-01-01T00:00:00Z
        let new_year = 1_483_228_800;
        assert_eq!(rfc3339("2016-12-31T23:59:60Z"), at(new_year, 0));
        assert_eq!(
            rfc3339("2016-12-31T23:59:60.25Z"),
            at(new_year, 250_000_000)
        );
        assert_eq!(rfc3339("2017-01-01T00:59:60+01:00"), at(new_year, 0));
        assert_eq!(rfc3339("2016-06-30T23:59:60Z"), at(1_467_331_200, 0));
        for text in [
            "2016-12-30T23:59:60Z",
            "2016-12-31T12:59:60Z",
            "2016-12-31T23:58:60Z",
            "2016-12-31T23:59:61Z",
        ] {
            assert_eq!(rfc3339(text), None, "{text}");
        }
    }

    #[test]
    fn c11_5_rfc3339_refuses_each_listed_form() {
        let long = format!("2024-08-20T18:37:24Z{}", " ".repeat(4097 - 20));
        assert_eq!(long.len(), 4097);
        let exact = format!("2024-08-20T18:37:24Z{}", " ".repeat(4096 - 20));
        assert_eq!(rfc3339(&exact), at(T, 0));
        for text in [
            long.as_str(),
            "2024-08-20x18:37:24Z",
            "2024-08-20_18:37:24Z",
            "2024-08-20  18:37:24Z",
            "2024-08-20",
            "2024-08-20T18:37Z",
            "2024-08-20T18:37:24",
            "1724179044",
            "",
            "+2024-08-20T18:37:24Z",
            "-2024-08-20T18:37:24Z",
            "2026-02-29T00:00:00Z",
            "2026-02-31T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-00-10T00:00:00Z",
            "2026-01-00T00:00:00Z",
            "2026-01-01T24:00:00Z",
            "2026-01-01T00:60:00Z",
            "2024-08-20T18:37:24+0200",
            "2024-08-20T18:37:24+2:00",
            "2024-08-20T18:37:24+24:00",
            "2024-08-20T18:37:24+01:60",
            "2024-08-20T18:37:24 UTC",
            "2024-08-20T18:37:24GMT",
            "2024-08-20T18:37:24.Z",
            "2024-08-20T18:37:24,5Z",
            "2024-08-20T18:37:24Z trailing",
        ] {
            assert_eq!(rfc3339(text), None, "{text:.40}");
        }
        assert_eq!(rfc3339("2028-02-29T00:00:00Z"), at(1_835_395_200, 0));
    }

    #[test]
    fn c11_5_rfc3339_before_1970_keeps_its_sign() {
        assert_eq!(
            rfc3339("1969-12-31T23:59:59Z"),
            UNIX_EPOCH.checked_sub(Duration::from_secs(1))
        );
        assert_eq!(
            rfc3339("1969-12-31T23:59:59.5Z"),
            UNIX_EPOCH.checked_sub(Duration::from_millis(500))
        );
        assert_eq!(
            rfc3339("0001-01-01T00:00:00Z"),
            UNIX_EPOCH.checked_sub(Duration::from_secs(62_135_596_800))
        );
    }

    #[test]
    fn c11_5_a_reset_further_than_fifty_years_is_none() {
        let base = UNIX_EPOCH + Duration::from_secs(T);
        assert_eq!(until(base + FIFTY_YEARS, base), Some(FIFTY_YEARS));
        assert_eq!(
            until(base + FIFTY_YEARS + Duration::from_nanos(1), base),
            None
        );
        assert_eq!(
            until(base - Duration::from_secs(1), base),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn c11_5_an_rfc_850_year_uses_the_fifty_year_rule_of_the_core() {
        // 2026-01-01T00:00:00Z: exactly fifty years ahead stays in 2076, one second more
        // goes back to 1976.
        let now = UNIX_EPOCH + Duration::from_secs(1_767_225_600);
        assert_eq!(
            http_date("Wednesday, 01-Jan-76 00:00:00 GMT", now),
            at(3_345_062_400, 0)
        );
        assert_eq!(
            http_date("Thursday, 01-Jan-76 00:00:01 GMT", now),
            at(189_302_401, 0)
        );
        assert_eq!(http_date("Wednesday, 01-Jan-76 00:00:01 GMT", now), None);
        assert_eq!(http_date("Sunday, 06-Nov-94 08:49:37 UTC", now), None);
        assert_eq!(http_date("Sun, 06-Nov-94 08:49:37 GMT", now), None);
    }

    #[test]
    fn c11_5_an_rfc_850_year_chooses_its_century_as_the_core_does_on_any_clock() {
        // Half a second before 1970: the clock's year is 1969, so `20` is 1920.
        let before = UNIX_EPOCH - Duration::from_millis(500);
        assert_eq!(http_date("Wednesday, 01-Jan-20 00:00:00 GMT", before), None);
        assert_eq!(
            http_date("Thursday, 01-Jan-20 00:00:00 GMT", before),
            UNIX_EPOCH.checked_sub(Duration::from_secs(1_577_923_200))
        );
        // A clock in the year 0 chooses no century.
        let year_zero = UNIX_EPOCH - Duration::from_secs(62_150_000_000);
        assert_eq!(http_date("Friday, 01-Jan-49 00:00:00 GMT", year_zero), None);
    }

    #[test]
    fn c11_5_date_header_is_read_in_the_three_http_forms() {
        let now = UNIX_EPOCH + Duration::from_secs(T);
        for text in [
            "Sun, 06 Nov 1994 08:49:37 GMT",
            " Sun, 06 Nov 1994 08:49:37 GMT\t",
            "Sunday, 06-Nov-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 1994",
        ] {
            assert_eq!(http_date(text, now), at(784_111_777, 0), "{text}");
        }
        assert_eq!(
            http_date("Sat, 31 Dec 2016 23:59:60 GMT", now),
            at(1_483_228_800, 0)
        );
        assert_eq!(
            http_date("Fri, 01 Jan 1960 00:00:00 GMT", now),
            UNIX_EPOCH.checked_sub(Duration::from_secs(315_619_200))
        );
        let long = format!("Sun, 06 Nov 1994 08:49:37 GMT{}", " ".repeat(4097 - 29));
        assert_eq!(http_date(&long, now), None);
        let exact = format!("Sun, 06 Nov 1994 08:49:37 GMT{}", " ".repeat(4096 - 29));
        assert_eq!(http_date(&exact, now), at(784_111_777, 0));
        for text in [
            "Mon, 06 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 nov 1994 08:49:37 GMT",
            "Sun, 6 Nov 1994 08:49:37 GMT",
            "Thu, 30 Feb 1995 08:49:37 GMT",
            "Sun, 06 Nov 1994 24:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:60 GMT",
            "784111777",
            "",
        ] {
            assert_eq!(http_date(text, now), None, "{text}");
        }
    }
}
