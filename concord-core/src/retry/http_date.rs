//! The three HTTP-date forms of RFC 9110 §5.6.7, using the decision's clock.

use std::time::{Duration, SystemTime};

fn number(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    // PANIC-SAFETY: All callers pass at most four ASCII digits; each digit subtraction is nonnegative.
    Some(bytes.iter().fold(0, |n, b| n * 10 + i64::from(b - b'0')))
}

fn month(bytes: &[u8]) -> Option<i64> {
    b"JanFebMarAprMayJunJulAugSepOctNovDec"
        // PANIC-SAFETY: The compile-time chunk size is the nonzero constant three.
        .as_chunks::<3>()
        .0
        .iter()
        .position(|name| name == bytes)
        // PANIC-SAFETY: The month position is 0..11; adding one fits i64.
        .map(|n| n as i64 + 1)
}

fn time(bytes: &[u8]) -> Option<(i64, i64, i64)> {
    if bytes.len() != 8 || *bytes.get(2)? != b':' || *bytes.get(5)? != b':' {
        return None;
    }
    let (h, m, s) = (
        number(bytes.get(..2)?)?,
        number(bytes.get(3..5)?)?,
        number(bytes.get(6..)?)?,
    );
    (h < 24 && m < 60 && (s < 60 || (h == 23 && m == 59 && s == 60))).then_some((h, m, s))
}

fn days_before_year(year: i64) -> i64 {
    // PANIC-SAFETY: All callers bound the year to 1..10000 before this helper is used.
    let y = year - 1;
    // PANIC-SAFETY: All callers bound the year to 1..10000 before this helper is used.
    365 * y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)
}

fn month_days(year: i64, month: i64) -> i64 {
    match month {
        // PANIC-SAFETY: The leap-day addend is zero or one; all divisors are positive constants.
        2 => 28 + i64::from(year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)),
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

type Date = (i64, i64, i64, i64, i64, i64);

fn seconds((year, month, day, h, m, s): Date) -> Option<i64> {
    if !(1..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || !(1..=month_days(year, month)).contains(&day)
    {
        return None;
    }
    // PANIC-SAFETY: The validated year is 1..9999, month 1..12 and day 1..31; the day sum fits i64.
    let days = days_before_year(year) - days_before_year(1970)
        // PANIC-SAFETY: The validated year is 1..9999, month 1..12 and day 1..31; the day sum fits i64.
        + (1..month).map(|m| month_days(year, m)).sum::<i64>()
        // PANIC-SAFETY: The validated year is 1..9999, month 1..12 and day 1..31; the day sum fits i64.
        + day
        // PANIC-SAFETY: The validated year is 1..9999, month 1..12 and day 1..31; the day sum fits i64.
        - 1;
    // PANIC-SAFETY: Validated four-digit dates and 24-hour time are far inside the i64 seconds range.
    Some(days * 86400 + h * 3600 + m * 60 + s)
}

fn clock_date(now: SystemTime) -> Option<Date> {
    let seconds = match now.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok()?,
        Err(e) => {
            let d = e.duration();
            // PANIC-SAFETY: try_from bounds seconds to i64::MAX; negation minus a 0/1 fraction reaches at worst i64::MIN.
            -i64::try_from(d.as_secs()).ok()? - i64::from(d.subsec_nanos() != 0)
        }
    };
    // PANIC-SAFETY: Dividing any i64 seconds by 86400 leaves ample headroom for the epoch-day offset.
    let days = seconds.div_euclid(86400) + days_before_year(1970);
    // Only four-digit years can occur in HTTP-date. Bound the search and arithmetic.
    if days < days_before_year(1) || days >= days_before_year(10000) {
        return None;
    }
    // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
    let mut year = 1 + days / 366;
    // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
    while days_before_year(year + 1) <= days {
        // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
        year += 1;
    }
    // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
    let mut day = days - days_before_year(year);
    let mut month = 1;
    while day >= month_days(year, month) {
        // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
        day -= month_days(year, month);
        // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
        month += 1;
    }
    let time = seconds.rem_euclid(86400);
    // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
    Some((year, month, day + 1, time / 3600, time / 60 % 60, time % 60))
}

pub(super) fn parse(text: &str, now: SystemTime) -> Option<SystemTime> {
    let bytes = text.as_bytes();
    let short_days: [&[u8]; 7] = [b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat", b"Sun"];
    let long_days: [&[u8]; 7] = [
        b"Monday",
        b"Tuesday",
        b"Wednesday",
        b"Thursday",
        b"Friday",
        b"Saturday",
        b"Sunday",
    ];
    let (date, weekday) = if bytes.len() == 29
        && bytes.get(3..5)? == b", "
        && *bytes.get(7)? == b' '
        && *bytes.get(11)? == b' '
        && *bytes.get(16)? == b' '
        && bytes.get(25..)? == b" GMT"
    {
        let weekday_text = bytes.get(..3)?;
        let weekday = short_days.iter().position(|&d| d == weekday_text)?;
        let (h, m, s) = time(bytes.get(17..25)?)?;
        (
            (
                number(bytes.get(12..16)?)?,
                month(bytes.get(8..11)?)?,
                number(bytes.get(5..7)?)?,
                h,
                m,
                s,
            ),
            weekday,
        )
    } else if let Some(comma) = bytes.iter().position(|&b| b == b',') {
        let weekday_text = bytes.get(..comma)?;
        let weekday = long_days.iter().position(|&d| d == weekday_text)?;
        // PANIC-SAFETY: position found a comma byte strictly before bytes.len(); adding one stays within the slice.
        let tail = bytes.get(comma + 1..)?;
        if tail.len() != 23
            || *tail.first()? != b' '
            || *tail.get(3)? != b'-'
            || *tail.get(7)? != b'-'
            || *tail.get(10)? != b' '
            || tail.get(19..)? != b" GMT"
        {
            return None;
        }
        let (h, m, s) = time(tail.get(11..19)?)?;
        let (month, day) = (month(tail.get(4..7)?)?, number(tail.get(1..3)?)?);
        let (now_year, now_month, now_day, now_h, now_m, now_s) = clock_date(now)?;
        // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
        let limit = (now_year + 50, now_month, now_day, now_h, now_m, now_s);
        // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
        let mut year = (now_year + 50) / 100 * 100 + number(tail.get(8..10)?)?;
        if (year, month, day, h, m, s) > limit {
            // PANIC-SAFETY: The preceding day bound limits the search to four-digit years, 12 months and at most 366 days.
            year -= 100;
        }
        ((year, month, day, h, m, s), weekday)
    } else if bytes.len() == 24
        && *bytes.get(3)? == b' '
        && *bytes.get(7)? == b' '
        && *bytes.get(10)? == b' '
        && *bytes.get(19)? == b' '
    {
        let weekday_text = bytes.get(..3)?;
        let weekday = short_days.iter().position(|&d| d == weekday_text)?;
        let day = if *bytes.get(8)? == b' ' {
            number(bytes.get(9..10)?)?
        } else {
            number(bytes.get(8..10)?)?
        };
        let (h, m, s) = time(bytes.get(11..19)?)?;
        (
            (
                number(bytes.get(20..)?)?,
                month(bytes.get(4..7)?)?,
                day,
                h,
                m,
                s,
            ),
            weekday,
        )
    } else {
        return None;
    };
    let (year, month, day, h, m, s) = date;
    let midnight = seconds((year, month, day, 0, 0, 0))?;
    // 1970-01-01 was Thursday; Monday is zero in the tables above.
    // Check the written date before a leap second rolls into the next day.
    // PANIC-SAFETY: Four-digit dates fit i64; weekday is a seven-element array position, 0..6.
    if (midnight.div_euclid(86400) + 3).rem_euclid(7) != weekday as i64 {
        return None;
    }
    // PANIC-SAFETY: Validated four-digit dates and 24-hour time are far inside the i64 seconds range.
    let seconds = midnight + h * 3600 + m * 60 + s;
    if seconds >= 0 {
        // PANIC-SAFETY: The surrounding branch requires seconds >= 0, so conversion to u64 is lossless.
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds as u64))
    } else {
        SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obsolete_years_use_the_fifty_year_boundary_in_both_centuries() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_794_441_600);
        // Exactly fifty years ahead stays in 2076; one second further rolls to 1976.
        assert_eq!(
            parse("Thursday, 12-Nov-76 00:00:00 GMT", now),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(3_372_364_800))
        );
        assert_eq!(
            parse("Friday, 12-Nov-76 00:00:01 GMT", now),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(216_604_801))
        );
        // Near a century's end, the nearest matching year can be in the next century.
        let now = parse("Sun, 01 Jan 2090 00:00:00 GMT", now).unwrap();
        assert_eq!(
            parse("Wednesday, 01-Jan-10 00:00:00 GMT", now),
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(4_417_977_600))
        );
    }

    #[test]
    fn leap_seconds_normalize_after_checking_the_original_weekday() {
        let now = SystemTime::UNIX_EPOCH;
        let next = parse("Sun, 01 Jan 2017 00:00:00 GMT", now);
        assert!(next.is_some());
        for text in [
            "Sat, 31 Dec 2016 23:59:60 GMT",
            "Saturday, 31-Dec-16 23:59:60 GMT",
            "Sat Dec 31 23:59:60 2016",
        ] {
            assert_eq!(parse(text, now), next, "{text}");
        }
        for text in [
            "Sun, 31 Dec 2016 23:59:60 GMT",
            "Sunday, 31-Dec-16 23:59:60 GMT",
            "Sun Dec 31 23:59:60 2016",
            "Sat, 31 Dec 2016 23:59:61 GMT",
        ] {
            assert_eq!(parse(text, now), None, "{text}");
        }
    }

    #[test]
    fn rejects_invalid_calendar_dates_times_and_grammar() {
        let now = SystemTime::UNIX_EPOCH;
        for text in [
            "",
            "nonsense",
            "Sun, 06 Nov 1994 08:49:37 UTC",
            "Sun, 06 Nov 1994 24:49:37 GMT",
            "Sun, 06 Nov 1994 08:60:37 GMT",
            "Sun, 06 Nov 1994 08:49:60 GMT",
            "Sun, 31 Nov 1994 08:49:37 GMT",
            "Sun, 29 Feb 1900 08:49:37 GMT",
            "Mon, 06 Nov 1994 08:49:37 GMT",
            "Sun, 06 Nov 1994 08:49:37 GMT junk",
            "Sun, 6 Nov 1994 08:49:37 GMT",
            "Sunday, 6-Nov-94 08:49:37 GMT",
            "Sun Nov 6 08:49:37 1994",
            "Sunday, 06-Xxx-94 08:49:37 GMT",
            "Sun Nov  6 08:49:37 abcd",
        ] {
            assert_eq!(parse(text, now), None, "{text}");
        }
    }

    #[test]
    fn clock_calendar_handles_leap_centuries_and_before_the_epoch() {
        for text in [
            "Tue, 29 Feb 2000 01:02:03 GMT",
            "Mon, 01 Mar 2100 01:02:03 GMT",
            "Wed, 31 Dec 1969 23:59:59 GMT",
            "Sat, 01 Jan 2000 00:00:00 GMT",
        ] {
            let date = parse(text, SystemTime::UNIX_EPOCH).unwrap();
            assert_eq!(
                seconds(clock_date(date).unwrap()).unwrap(),
                match date.duration_since(SystemTime::UNIX_EPOCH) {
                    Ok(d) => d.as_secs() as i64,
                    Err(e) => -(e.duration().as_secs() as i64),
                }
            );
        }
    }
}
