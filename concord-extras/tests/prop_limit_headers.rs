//! C11.5 and C4.3: the readers of rate-limit headers, the RFC 3339 reset and the two readers
//! of HTTP dates (`Date` here, `Retry-After` in the core), against models of their grammars.
#![cfg(feature = "limit")]
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord_core::http::{HeaderMap, HeaderName, HeaderValue};
use concord_core::reqwest::{self, Method, Request};
use concord_core::transport::TryInfo;
use concord_core::{fake, Headers, Middleware, Next};
use concord_extras::limit::{counts, remaining, remaining_rfc3339, Bucket, Limit, Seen, Window};
use proptest::prelude::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[path = "corpus/read.rs"]
mod corpus;

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}

/// Fifty years of 365.2425 days, the furthest a reset is read.
const FIFTY_YEARS: u64 = 1_577_847_600;
/// The first and the last second of the years 1 to 9999.
const FIRST: i64 = -62_135_596_800;
const LAST: i64 = 253_402_300_799;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .unwrap()
}

/// Calls `look` with the `Seen` of one response carrying `headers`, through a real `Limit`.
/// `look` runs inside the limit, which catches a panic: its result is checked outside.
fn seen<R: Send + 'static>(
    rt: &tokio::runtime::Runtime,
    headers: HeaderMap,
    look: impl Fn(&Seen<'_>) -> R + Send + Sync + 'static,
) -> R {
    let out = Arc::new(Mutex::new(None));
    let kept = out.clone();
    let limit = Limit::new().bucket(Bucket::new(|_| Some("k".into())).learn(move |seen| {
        *kept.lock().unwrap() = Some(look(seen));
        Vec::new()
    }));
    let transport = fake(move |_| {
        let mut response = concord_core::http::Response::new(reqwest::Body::from(""));
        *response.headers_mut() = headers.clone();
        Ok(response.into())
    });
    let request = Request::new(Method::GET, "https://one.test/data".parse().unwrap());
    rt.block_on(limit.handle(request, TryInfo::new("props", 1), Next::to(&transport)))
        .unwrap();
    // Nothing here means that the learn function panicked, and the limit caught it.
    let result = out.lock().unwrap().take();
    result.unwrap()
}

fn header(map: &mut HeaderMap, name: &str, value: &[u8]) {
    if let Ok(value) = HeaderValue::from_bytes(value) {
        map.append(HeaderName::from_bytes(name.as_bytes()).unwrap(), value);
    }
}

// ---- Calendar, written from the proleptic Gregorian rules, for the models. ----

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}
fn month_days(year: i64, month: i64) -> i64 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}
/// (year, month, day, hour, minute, second) of a count of seconds from 1970.
fn calendar(seconds: i64) -> (i64, i64, i64, i64, i64, i64) {
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let rest = seconds.rem_euclid(86_400);
    (year, month, day, rest / 3600, rest / 60 % 60, rest % 60)
}
fn system(nanos: i128) -> SystemTime {
    let size = Duration::new(
        (nanos.unsigned_abs() / 1_000_000_000) as u64,
        (nanos.unsigned_abs() % 1_000_000_000) as u32,
    );
    if nanos < 0 {
        UNIX_EPOCH - size
    } else {
        UNIX_EPOCH + size
    }
}
/// A wait of this many nanoseconds, zero when it is past.
fn duration(nanos: i128) -> Duration {
    let nanos = nanos.max(0);
    Duration::new(
        (nanos / 1_000_000_000) as u64,
        (nanos % 1_000_000_000) as u32,
    )
}
fn nanos_of(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(after) => after.as_nanos() as i128,
        Err(before) => -(before.duration().as_nanos() as i128),
    }
}

const SHORT: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const LONG: [&str; 7] = [
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
fn weekday(days: i64) -> usize {
    (days + 3).rem_euclid(7) as usize
}

// ---- The HTTP-date grammar of RFC 9110, section 5.6.7, as C4.3 and C11.5 read it. ----

fn digits(text: &str, width: usize) -> Option<i64> {
    (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}
fn clock(text: &str) -> Option<(i64, i64, i64)> {
    let bytes = text.as_bytes();
    if bytes.len() != 8 || bytes[2] != b':' || bytes[5] != b':' {
        return None;
    }
    let (h, m, s) = (
        digits(&text[..2], 2)?,
        digits(&text[3..5], 2)?,
        digits(&text[6..], 2)?,
    );
    (h < 24 && m < 60 && (s < 60 || (h, m, s) == (23, 59, 60))).then_some((h, m, s))
}
fn month(text: &str) -> Option<i64> {
    MONTHS.iter().position(|m| *m == text).map(|m| m as i64 + 1)
}
/// The seconds from 1970 of an HTTP date, its two-digit year read against `now`.
fn http_date_model(text: &str, now: SystemTime) -> Option<i64> {
    let text = text.trim_matches([' ', '\t']);
    if !text.is_ascii() {
        return None;
    }
    let b = text.as_bytes();
    let (name, year, mon, day, time) = if b.len() == 29
        && &text[3..5] == ", "
        && b[7] == b' '
        && b[11] == b' '
        && b[16] == b' '
        && &text[25..] == " GMT"
    {
        (
            &text[..3],
            digits(&text[12..16], 4)?,
            month(&text[8..11])?,
            digits(&text[5..7], 2)?,
            clock(&text[17..25])?,
        )
    } else if let Some((name, rest)) = text.split_once(", ") {
        let r = rest.as_bytes();
        if r.len() != 22 || r[2] != b'-' || r[6] != b'-' || r[9] != b' ' || &rest[18..] != " GMT" {
            return None;
        }
        if !LONG.contains(&name) {
            return None;
        }
        let (mon, day, time) = (
            month(&rest[3..6])?,
            digits(&rest[..2], 2)?,
            clock(&rest[10..18])?,
        );
        let two = digits(&rest[7..9], 2)?;
        // The latest year that is not more than fifty years after the clock's date.
        let now = nanos_of(now).div_euclid(1_000_000_000) as i64;
        let (ny, nm, nd, nh, nmi, ns) = calendar(now);
        if !(1..=9999).contains(&ny) {
            return None;
        }
        let mut year = (ny + 50) / 100 * 100 + two;
        if (year, mon, day, time.0, time.1, time.2) > (ny + 50, nm, nd, nh, nmi, ns) {
            year -= 100;
        }
        (name, year, mon, day, time)
    } else if b.len() == 24 && b[3] == b' ' && b[7] == b' ' && b[10] == b' ' && b[19] == b' ' {
        let day = &text[8..10];
        let day = match day.strip_prefix(' ') {
            Some(one) => digits(one, 1)?,
            None => digits(day, 2)?,
        };
        (
            &text[..3],
            digits(&text[20..], 4)?,
            month(&text[4..7])?,
            day,
            clock(&text[11..19])?,
        )
    } else {
        return None;
    };
    if !(1..=9999).contains(&year) || day < 1 || day > month_days(year, mon) {
        return None;
    }
    let days = days_from_civil(year, mon, day);
    let names: &[&str] = if name.len() == 3 { &SHORT } else { &LONG };
    if names[weekday(days)] != name {
        return None;
    }
    Some(days * 86_400 + time.0 * 3600 + time.1 * 60 + time.2)
}

/// An HTTP date in one of its three forms; the weekday of `wrong` days later when asked.
fn http_text(seconds: i64, form: u8, wrong: u8, padded: bool) -> String {
    let (y, mo, d, h, mi, s) = calendar(seconds);
    let days = seconds.div_euclid(86_400);
    let w = (weekday(days) + usize::from(wrong % 7)) % 7;
    let m = MONTHS[(mo - 1) as usize];
    match form % 3 {
        0 => format!("{}, {d:02} {m} {y:04} {h:02}:{mi:02}:{s:02} GMT", SHORT[w]),
        1 => format!(
            "{}, {d:02}-{m}-{:02} {h:02}:{mi:02}:{s:02} GMT",
            LONG[w],
            y % 100
        ),
        _ if padded => format!("{} {m} {d:02} {h:02}:{mi:02}:{s:02} {y:04}", SHORT[w]),
        _ => format!("{} {m} {d:>2} {h:02}:{mi:02}:{s:02} {y:04}", SHORT[w]),
    }
}
/// The same, at 23:59:60 of the day of `seconds`.
fn leap_text(seconds: i64, form: u8) -> String {
    let start = seconds.div_euclid(86_400) * 86_400;
    http_text(start + 86_399, form, 0, false).replace("23:59:59", "23:59:60")
}

/// Changes to a text: none, or one that the grammar may or may not allow.
fn mutate(text: &str, how: u8, at: usize, byte: u8) -> Vec<u8> {
    let mut out = text.as_bytes().to_vec();
    let at = if out.is_empty() { 0 } else { at % out.len() };
    match how % 16 {
        0..=5 => {}
        6 if !out.is_empty() => out[at] = byte,
        7 if !out.is_empty() => {
            out.remove(at);
        }
        8 => out.insert(at, byte),
        9 => out = format!(" \t{text}\t ").into_bytes(),
        10 => out = text.to_ascii_lowercase().into_bytes(),
        11 => out = text.replace("GMT", "UTC").into_bytes(),
        12 => {
            // To 4,096 bytes and one more, with spaces that are trimmed.
            let width = 4096 + usize::from(byte.is_multiple_of(2));
            out.resize(width.max(out.len()), b' ');
        }
        13 => out = format!("{text} ").repeat(2).into_bytes(),
        14 => out.push(0x80 | byte),
        _ => out = text.replace(':', "").into_bytes(),
    }
    out
}

fn date_seconds() -> impl Strategy<Value = i64> {
    let now = nanos_of(SystemTime::now()).div_euclid(1_000_000_000) as i64;
    prop_oneof![
        2 => FIRST..=LAST,
        // Near this clock, where a two-digit year can be read.
        3 => (now - 60 * 31_557_600)..(now + 60 * 31_557_600),
        1 => prop::sample::select(vec![FIRST, LAST, 0, -1, 951_782_400, 4_107_542_400]),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    /// Also proves C4.3.
    fn c11_5_the_two_readers_of_http_dates_agree_except_for_the_bound_of_date(
        seconds in date_seconds(), form in any::<u8>(), wrong in prop_oneof![4 => Just(0u8), 1 => any::<u8>()],
        leap in prop::bool::weighted(0.1), padded in any::<bool>(),
        how in any::<u8>(), at in any::<usize>(), byte in 0x20u8..0x7f, ahead in 0u64..=FIFTY_YEARS,
    ) {
        let text = if leap { leap_text(seconds, form) } else { http_text(seconds, form, wrong, padded) };
        let written = mutate(&text, how, at, byte);
        let mut headers = HeaderMap::new();
        header(&mut headers, "date", &written);
        let probe = written.clone();
        let rt = runtime();
        // Inside the limit: the local clock, the model's date, and what `Date` made the reset.
        let (now, model, until, probed) = seen(&rt, headers, move |seen| {
            let now = seen.now();
            let text = std::str::from_utf8(&probe).ok();
            let model = text.and_then(|text| http_date_model(text, now));
            // A reset `ahead` after the model's date, within the year 9999, or 1,000 seconds
            // after this clock.
            let reset = match model {
                Some(date) => i128::from(date + (ahead as i64).min(LAST - date)) * 1_000_000_000,
                None => i128::from(nanos_of(now).div_euclid(1_000_000_000) as i64 + 1_000) * 1_000_000_000,
            };
            (now, model, seen.until_rfc3339(&rfc3339_text(reset)), reset)
        });
        let text = std::str::from_utf8(&written).ok().filter(|_| HeaderValue::from_bytes(&written).is_ok());
        // `Date`: read only up to 4,096 bytes; otherwise the reset counts from this clock.
        let bounded = model.filter(|_| written.len() <= 4096);
        let from = match bounded {
            Some(date) => i128::from(date) * 1_000_000_000,
            None => nanos_of(now),
        };
        let expected = (probed - from <= i128::from(FIFTY_YEARS) * 1_000_000_000).then(|| duration(probed - from));
        prop_assert_eq!(until, expected, "{:?}", String::from_utf8_lossy(&written));
        // `Retry-After`, read by the core with the same clock.
        let Some(text) = text else { return Ok(()); };
        let trimmed = text.trim();
        let mut map = HeaderMap::new();
        map.insert("retry-after", HeaderValue::from_str(text).unwrap());
        let headers = Headers::from(map);
        if !trimmed.is_empty() && trimmed.bytes().all(|b| b.is_ascii_digit()) {
            return Ok(()); // A number of seconds, not a date.
        }
        let core = headers.retry_after_at(now);
        let expected = model.map(|date| duration(i128::from(date) * 1_000_000_000 - nanos_of(now)));
        prop_assert_eq!(core, expected, "{:?}", text);
        if let Some(date) = model {
            // The exact date: from the second before it, the core waits one second.
            let before = system((date as i128 - 1) * 1_000_000_000);
            prop_assert_eq!(headers.retry_after_at(before), Some(sec(1)), "{:?}", text);
        }
        // The two readers agree, but for the bound of `Date`.
        prop_assert_eq!(core.is_some(), bounded.is_some() || written.len() > 4096 && model.is_some());
    }
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c4_3_an_rfc_850_year_reads_by_the_fifty_year_rule_on_any_clock(
        clock in (FIRST + 86_400)..(LAST - 101 * 31_622_400), nanos in prop_oneof![Just(0u32), 0u32..1_000_000_000],
        near in prop_oneof![3 => -1i64..=1, 1 => -3_200_000_000i64..3_200_000_000],
    ) {
        // The date fifty calendar years after the clock, at the clock's time of day, and
        // the seconds around it: the last that keeps its century, and the first that does not.
        let (y, mo, d, h, mi, sec_) = calendar(clock);
        let day = d.min(month_days(y + 50, mo));
        let edge = days_from_civil(y + 50, mo, day) * 86_400 + h * 3600 + mi * 60 + sec_;
        let at = edge + near;
        let text = http_text(at, 1, 0, false);
        let now = system(i128::from(clock) * 1_000_000_000 + i128::from(nanos));
        let model = http_date_model(&text, now);
        let mut map = HeaderMap::new();
        map.insert("retry-after", HeaderValue::from_str(&text).unwrap());
        let headers = Headers::from(map);
        prop_assert_eq!(
            headers.retry_after_at(now),
            model.map(|date| duration(i128::from(date) * 1_000_000_000 - nanos_of(now))),
            "{:?} at {}", text, clock
        );
        if let Some(date) = model {
            prop_assert_eq!(headers.retry_after_at(system((i128::from(date) - 1) * 1_000_000_000)), Some(sec(1)));
        }
    }
}

// ---- RFC 3339, as C11.5 and `Seen::until_rfc3339` read it. ----

/// A date-time in UTC, written with `Z`.
fn rfc3339_text(nanos: i128) -> String {
    let seconds = nanos.div_euclid(1_000_000_000) as i64;
    let fraction = nanos.rem_euclid(1_000_000_000);
    let (y, mo, d, h, mi, s) = calendar(seconds);
    if fraction == 0 {
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
    } else {
        format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}.{fraction:09}Z")
    }
}

/// The nanoseconds from 1970 that the grammar of C11.5 gives a text.
fn rfc3339_model(text: &str) -> Option<i128> {
    if text.len() > 4096 {
        return None;
    }
    let text = text.trim_matches([' ', '\t']);
    if !text.is_ascii() || text.len() < 20 {
        return None;
    }
    let b = text.as_bytes();
    if b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't' | b' ')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let (year, mon, day) = (
        digits(&text[..4], 4)?,
        digits(&text[5..7], 2)?,
        digits(&text[8..10], 2)?,
    );
    let (hour, minute, second) = (
        digits(&text[11..13], 2)?,
        digits(&text[14..16], 2)?,
        digits(&text[17..19], 2)?,
    );
    let mut rest = &text[19..];
    let mut fraction = 0i128;
    let mut finer = false;
    if let Some(after) = rest.strip_prefix('.') {
        let count = after.bytes().take_while(u8::is_ascii_digit).count();
        if count == 0 {
            return None;
        }
        let figures = &after[..count];
        for (n, digit) in figures.bytes().enumerate() {
            if n < 9 {
                fraction = fraction * 10 + i128::from(digit - b'0');
            } else if digit != b'0' {
                finer = true;
            }
        }
        fraction *= 10i128.pow(9u32.saturating_sub(count as u32));
        rest = &after[count..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let r = rest.as_bytes();
            if r.len() != 6 || !matches!(r[0], b'+' | b'-') || r[3] != b':' {
                return None;
            }
            let (h, m) = (digits(&rest[1..3], 2)?, digits(&rest[4..], 2)?);
            if h > 23 || m > 59 {
                return None;
            }
            (h * 3600 + m * 60) * if r[0] == b'-' { -1 } else { 1 }
        }
    };
    if !(1..=12).contains(&mon)
        || day < 1
        || day > month_days(year, mon)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }
    let local = days_from_civil(year, mon, day) * 86_400 + hour * 3600 + minute * 60;
    let mut utc = local + second.min(59) - offset;
    if second == 60 {
        // Only the last second of a month in UTC, which counts as the next second.
        let (_, _, d, h, mi, s) = calendar(utc + 1);
        // The next second must also be one that the grammar can write: not in the year 10000.
        if (d, h, mi, s) != (1, 0, 0, 0) || utc + 1 > LAST {
            return None;
        }
        utc += 1;
    }
    Some(i128::from(utc) * 1_000_000_000 + fraction + i128::from(finer))
}

#[derive(Clone, Debug)]
struct Parts {
    seconds: i64,
    fraction: String,
    separator: &'static str,
    offset: String,
    leap: bool,
}
fn parts() -> impl Strategy<Value = Parts> {
    (
        prop_oneof![
            3 => -62_167_219_200i64..=LAST,
            1 => prop::sample::select(vec![-62_167_219_200i64, FIRST, 0, -1, LAST, 1_483_228_799, 1_467_331_199]),
        ],
        prop_oneof![
            3 => Just(String::new()),
            2 => "\\.[0-9]{1,12}",
            1 => prop::sample::select(vec![".", ".999999999", ".9999999999", ".0000000001", ".1234567890000", ",5"]).prop_map(str::to_owned),
        ],
        prop::sample::select(vec!["T", "T", "t", " ", "  ", "_", "x", ""]),
        prop_oneof![
            3 => prop::sample::select(vec!["Z", "z", "+00:00", "-00:00"]).prop_map(str::to_owned),
            2 => (any::<bool>(), 0u32..26, 0u32..62).prop_map(|(minus, h, m)| format!("{}{h:02}:{m:02}", if minus { '-' } else { '+' })),
            1 => prop::sample::select(vec!["", "+0200", "+2:00", " UTC", "GMT", "Z ", "ZZ"]).prop_map(str::to_owned),
        ],
        prop::bool::weighted(0.15),
    )
        .prop_map(|(seconds, fraction, separator, offset, leap)| Parts { seconds, fraction, separator, offset, leap })
}
fn offset_seconds(offset: &str) -> i64 {
    let b = offset.as_bytes();
    if b.len() == 6 && matches!(b[0], b'+' | b'-') {
        let h: i64 = offset[1..3].parse().unwrap_or(0);
        let m: i64 = offset[4..6].parse().unwrap_or(0);
        (h * 3600 + m * 60) * if b[0] == b'-' { -1 } else { 1 }
    } else {
        0
    }
}
fn render_parts(parts: &Parts) -> String {
    // The local time that the offset gives the instant.
    let local = parts.seconds + offset_seconds(&parts.offset);
    let (y, mo, d, h, mi, s) = calendar(local);
    let s = if parts.leap { 60 } else { s };
    format!(
        "{y:04}-{mo:02}-{d:02}{}{h:02}:{mi:02}:{s:02}{}{}",
        parts.separator, parts.fraction, parts.offset
    )
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c11_5_an_rfc3339_reset_reads_as_the_grammar_says(
        parts in parts(), how in any::<u8>(), at in any::<usize>(), byte in 0x20u8..0x7f,
        back in prop_oneof![3 => 0u64..=FIFTY_YEARS, 1 => (FIFTY_YEARS - 2)..(FIFTY_YEARS + 2)],
    ) {
        let text = render_parts(&parts);
        let written = mutate(&text, how, at, byte);
        let model = std::str::from_utf8(&written).ok().and_then(rfc3339_model);
        // Count from a server date `back` seconds before the reset, or before what was written.
        let anchor = model.unwrap_or(i128::from(parts.seconds) * 1_000_000_000);
        let base = (anchor.div_euclid(1_000_000_000) as i64 - back as i64).clamp(FIRST, LAST);
        let mut headers = HeaderMap::new();
        header(&mut headers, "date", http_text(base, 0, 0, false).as_bytes());
        header(&mut headers, "x-reset", &written);
        let rt = runtime();
        let until = seen(&rt, headers, |seen| {
            seen.header("x-reset").map(|text| seen.until_rfc3339(text))
        });
        let Some(until) = until else {
            // Not a header value that `Seen::header` returns: nothing to read.
            prop_assert!(std::str::from_utf8(&written).is_err() || HeaderValue::from_bytes(&written).map_or(true, |v| v.to_str().is_err()));
            return Ok(());
        };
        let expected = model.and_then(|at| {
            let ahead = (at - i128::from(base) * 1_000_000_000).max(0);
            (ahead <= i128::from(FIFTY_YEARS) * 1_000_000_000).then(|| Duration::from_nanos(ahead as u64))
        });
        prop_assert_eq!(until, expected, "{:?} from {}", String::from_utf8_lossy(&written), base);
    }
}

// ---- `counts`, `remaining` and `remaining_rfc3339`: the numbers and the lists. ----

/// A nonnegative integer as C11.5 reads it: digits only, around spaces and tabs.
fn number_model(text: &str) -> Option<u64> {
    let text = text.trim_matches([' ', '\t']);
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}
fn pairs_model(text: &str) -> Option<Vec<(u64, u64)>> {
    if text.len() > 4096 {
        return None;
    }
    let mut pairs = Vec::new();
    for pair in text.split(',') {
        let (n, seconds) = pair.split_once(':')?;
        let (n, seconds) = (number_model(n)?, number_model(seconds)?);
        if seconds == 0 || pairs.iter().any(|(_, old)| *old == seconds) {
            return None;
        }
        pairs.push((n, seconds));
    }
    if pairs.len() > 8 {
        return None;
    }
    pairs.sort_by_key(|(_, seconds)| *seconds);
    Some(pairs)
}
/// The windows that `counts` learns: capacity, period and what is left.
fn counts_model(limits: &str, used: &str) -> Vec<(u64, u64, u64)> {
    let (Some(limits), Some(used)) = (pairs_model(limits), pairs_model(used)) else {
        return Vec::new();
    };
    if limits.len() != used.len()
        || limits
            .iter()
            .zip(&used)
            .any(|((capacity, a), (_, b))| *capacity == 0 || a != b)
    {
        return Vec::new();
    }
    limits
        .iter()
        .zip(&used)
        .map(|((capacity, seconds), (count, _))| {
            (*capacity, *seconds, capacity.saturating_sub(*count))
        })
        .collect()
}

/// A number as a server might write it: digits of every length, signs, leading zeros,
/// spaces and tabs around, and near misses.
fn number_text() -> impl Strategy<Value = String> {
    prop_oneof![
        4 => (0u64..8).prop_map(|n| n.to_string()),
        2 => "[0-9]{1,40}",
        1 => ("0{1,6}", 0u64..8).prop_map(|(zeros, n)| format!("{zeros}{n}")),
        1 => (prop::sample::select(vec![" ", "\t", "  ", " \t"]), 0u64..8, prop::sample::select(vec!["", " ", "\t"]))
            .prop_map(|(before, n, after)| format!("{before}{n}{after}")),
        1 => prop::sample::select(vec![
            "", "+1", "-1", "1.0", "1e3", "0x10", "1 2", "١", "18446744073709551615",
            "18446744073709551616", "00000000000000000000000000000000000000001",
        ]).prop_map(str::to_owned),
    ]
}
fn list_text() -> impl Strategy<Value = String> {
    (
        proptest::collection::vec(
            (
                number_text(),
                prop_oneof![4 => (1u64..6).prop_map(|n| n.to_string()), 1 => number_text()],
                prop::sample::select(vec![":", ":", " : ", ";", ""]),
            ),
            0..11,
        ),
        prop::sample::select(vec![",", ",", ", ", " ,\t", ";"]),
        prop::option::of(prop::sample::select(vec![",", " ", "\t", "x"])),
        prop::option::of(4090usize..4100),
    )
        .prop_map(|(pairs, comma, tail, width)| {
            let mut text = pairs
                .iter()
                .map(|(n, seconds, colon)| format!("{n}{colon}{seconds}"))
                .collect::<Vec<_>>()
                .join(comma);
            if let Some(tail) = tail {
                text.push_str(tail);
            }
            if let Some(width) = width {
                while text.len() < width {
                    text.insert(0, ' ');
                }
            }
            text
        })
}
/// The limits and counts of one bucket, often with the same periods in a different order.
fn count_lists() -> impl Strategy<Value = (String, String)> {
    prop_oneof![
        (list_text(), list_text()),
        (
            proptest::collection::btree_set(1u64..9, 1..10),
            proptest::collection::vec((number_text(), number_text()), 10),
            any::<prop::sample::Index>()
        )
            .prop_map(|(periods, numbers, turn)| {
                let periods: Vec<u64> = periods.into_iter().collect();
                let limits: Vec<String> = periods
                    .iter()
                    .zip(&numbers)
                    .map(|(p, (c, _))| format!("{c}:{p}"))
                    .collect();
                let mut counts: Vec<String> = periods
                    .iter()
                    .zip(&numbers)
                    .map(|(p, (_, u))| format!("{u}:{p}"))
                    .collect();
                let turn = turn.index(counts.len());
                counts.rotate_left(turn);
                (limits.join(","), counts.join(", "))
            }),
        // Periods that may repeat, the same in both lists.
        proptest::collection::vec((1u64..4, 1u64..9, 0u64..9), 1..5).prop_map(|windows| {
            let limits: Vec<String> = windows.iter().map(|(p, c, _)| format!("{c}:{p}")).collect();
            let counts: Vec<String> = windows.iter().map(|(p, _, u)| format!("{u}:{p}")).collect();
            (limits.join(","), counts.join(","))
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, ..ProptestConfig::default() })]

    #[test]
    fn c11_5_counts_learns_the_windows_that_the_grammar_gives(
        lists in proptest::collection::vec(count_lists(), 1..12),
    ) {
        let mut headers = HeaderMap::new();
        for (n, (limits, used)) in lists.iter().enumerate() {
            header(&mut headers, &format!("x-limit-{n}"), limits.as_bytes());
            header(&mut headers, &format!("x-count-{n}"), used.as_bytes());
        }
        let count = lists.len();
        let rt = runtime();
        let learned = seen(&rt, headers, move |seen| {
            (0..count).map(|n| counts(format!("x-limit-{n}"), format!("x-count-{n}"))(seen).len()).collect::<Vec<_>>()
        });
        for (n, (limits, used)) in lists.iter().enumerate() {
            prop_assert_eq!(learned[n], counts_model(limits, used).len(), "{:?} {:?}", limits, used);
        }
    }

    #[test]
    fn c11_5_remaining_learns_one_window_when_the_grammar_allows(
        triples in proptest::collection::vec((number_text(), number_text(), prop_oneof![
            // A time from now, or any text written as it is.
            3 => (-5i64..1_000_000).prop_map(Ok),
            1 => number_text().prop_map(Err),
        ]), 1..12),
    ) {
        let now = nanos_of(SystemTime::now()).div_euclid(1_000_000_000) as i64;
        let mut headers = HeaderMap::new();
        let mut resets = Vec::new();
        for (n, (capacity, left, reset)) in triples.iter().enumerate() {
            let reset = match reset {
                Ok(ahead) => (now + ahead).to_string(),
                Err(text) => text.clone(),
            };
            header(&mut headers, &format!("x-limit-{n}"), capacity.as_bytes());
            header(&mut headers, &format!("x-left-{n}"), left.as_bytes());
            header(&mut headers, &format!("x-reset-{n}"), reset.as_bytes());
            resets.push(reset);
        }
        let count = triples.len();
        let rt = runtime();
        let (learned, now) = seen(&rt, headers, move |seen| {
            ((0..count)
                .map(|n| remaining(format!("x-limit-{n}"), format!("x-left-{n}"), format!("x-reset-{n}"))(seen).len())
                .collect::<Vec<_>>(), seen.now())
        });
        for (n, (capacity, left, _)) in triples.iter().enumerate() {
            let expected = match (number_model(capacity), number_model(left), number_model(&resets[n])) {
                (Some(capacity), Some(left), Some(reset)) => {
                    let future = UNIX_EPOCH
                        .checked_add(sec(reset))
                        .is_some_and(|at| at.duration_since(now).is_ok_and(|d| !d.is_zero()));
                    usize::from(capacity > 0 && left <= capacity && future)
                }
                _ => 0,
            };
            prop_assert_eq!(learned[n], expected, "{:?} {:?} {:?}", capacity, left, resets[n]);
        }
    }

    #[test]
    fn c11_5_remaining_rfc3339_reads_its_numbers_as_remaining_does(
        pairs in proptest::collection::vec((number_text(), number_text()), 1..12), ahead in 1u64..100_000,
    ) {
        // A server clock unrelated to this one, and a reset after it.
        let server = 1_893_456_000i64;
        let mut headers = HeaderMap::new();
        header(&mut headers, "date", http_text(server, 0, 0, false).as_bytes());
        for (n, (capacity, left)) in pairs.iter().enumerate() {
            header(&mut headers, &format!("x-limit-{n}"), capacity.as_bytes());
            header(&mut headers, &format!("x-left-{n}"), left.as_bytes());
        }
        header(&mut headers, "x-reset", rfc3339_text(i128::from(server + ahead as i64) * 1_000_000_000).as_bytes());
        let count = pairs.len();
        let rt = runtime();
        let learned = seen(&rt, headers, move |seen| {
            (0..count)
                .map(|n| remaining_rfc3339(format!("x-limit-{n}"), format!("x-left-{n}"), "x-reset")(seen).len())
                .collect::<Vec<_>>()
        });
        for (n, (capacity, left)) in pairs.iter().enumerate() {
            let expected = match (number_model(capacity), number_model(left)) {
                (Some(capacity), Some(left)) => usize::from(capacity > 0 && left <= capacity),
                _ => 0,
            };
            prop_assert_eq!(learned[n], expected, "{:?} {:?}", capacity, left);
        }
    }
}

/// When each of seven calls left, or that it was refused, under one learning function.
fn schedule(
    headers: HeaderMap,
    learn: impl Fn(&Seen<'_>) -> Vec<Window> + Send + Sync + 'static,
) -> Vec<Option<Duration>> {
    let rt = runtime();
    rt.block_on(async move {
        let limit = Limit::new()
            .max_hold(sec(30))
            .bucket(Bucket::new(|_| Some("k".into())).learn(learn));
        let first = Arc::new(Mutex::new(Some(headers)));
        let transport = fake(move |_| {
            let mut response = concord_core::http::Response::new(reqwest::Body::from(""));
            if let Some(headers) = first.lock().unwrap().take() {
                *response.headers_mut() = headers;
            }
            Ok(response.into())
        });
        let start = tokio::time::Instant::now();
        let mut times = Vec::new();
        for _ in 0..7 {
            let request = Request::new(Method::GET, "https://one.test/data".parse().unwrap());
            let sent = limit
                .handle(request, TryInfo::new("props", 1), Next::to(&transport))
                .await;
            times.push(sent.ok().map(|_| start.elapsed()));
        }
        times
    })
}

proptest! {
    #![proptest_config(ProptestConfig { failure_persistence: None, cases: ProptestConfig::default().cases / 4, ..ProptestConfig::default() })]

    #[test]
    fn c11_5_counts_holds_calls_as_the_windows_it_reads(
        windows in proptest::collection::btree_map(1u64..6, (1u64..4, 0u64..5), 1..3),
        zeros in proptest::collection::vec("0{0,3}", 4), space in prop::sample::select(vec!["", " ", "\t"]),
    ) {
        // The same numbers, written with leading zeros and spaces, and the windows they give.
        let limits: Vec<String> = windows.iter().enumerate().map(|(n, (p, (c, _)))| format!("{space}{}{c}:{p}", zeros[n % 4])).collect();
        let used: Vec<String> = windows.iter().rev().enumerate().map(|(n, (p, (_, u)))| format!("{}{u}{space}:{p}", zeros[(n + 1) % 4])).collect();
        let mut headers = HeaderMap::new();
        header(&mut headers, "x-limit", limits.join(",").as_bytes());
        header(&mut headers, "x-count", used.join(",").as_bytes());
        let read = schedule(headers.clone(), counts("x-limit", "x-count"));
        let given: Vec<Window> = windows
            .iter()
            .map(|(p, (c, u))| Window::rolling(*c, sec(*p)).remaining(c.saturating_sub(*u)))
            .collect();
        let made = schedule(headers, move |seen| if seen.header("x-limit").is_some() { given.clone() } else { Vec::new() });
        prop_assert_eq!(read, made);
    }

    #[test]
    fn c11_5_remaining_holds_calls_as_the_window_it_reads(
        (capacity, left) in (1u64..4).prop_flat_map(|capacity| (Just(capacity), 0..=capacity)),
        zeros in "0{0,3}", space in prop::sample::select(vec!["", " ", "\t"]),
    ) {
        let reset = nanos_of(SystemTime::now()).div_euclid(1_000_000_000) as u64 + 100_000;
        let mut headers = HeaderMap::new();
        header(&mut headers, "x-limit", format!("{space}{zeros}{capacity}").as_bytes());
        header(&mut headers, "x-left", format!("{zeros}{left}{space}").as_bytes());
        header(&mut headers, "x-reset", format!("{zeros}{reset}").as_bytes());
        let read = schedule(headers.clone(), remaining("x-limit", "x-left", "x-reset"));
        let made = schedule(headers, move |seen| match seen.header("x-limit") {
            Some(_) => vec![Window::fixed(capacity, seen.until_unix(reset).unwrap()).remaining(left)],
            None => Vec::new(),
        });
        prop_assert_eq!(read, made);
    }
}

/// What `Date` makes of a reset one second after `at`, within the year 9999, or a thousand
/// seconds after this clock.
fn date_reader(text: &[u8], at: Option<i64>, rt: &tokio::runtime::Runtime) -> Option<Duration> {
    let mut headers = HeaderMap::new();
    header(&mut headers, "date", text);
    seen(rt, headers, move |seen| {
        let reset = at.map_or_else(
            || (nanos_of(seen.now()).div_euclid(1_000_000_000) + 1000) * 1_000_000_000,
            |at| i128::from((at + 1).min(LAST)) * 1_000_000_000,
        );
        seen.until_rfc3339(&rfc3339_text(reset))
    })
}
/// What `Retry-After` makes of the text, from this clock and from the second before `at`.
fn retry_after(
    text: &[u8],
    at: Option<i64>,
    clock: SystemTime,
) -> (Option<Duration>, Option<Duration>) {
    let Ok(value) = HeaderValue::from_bytes(text) else {
        return (None, None);
    };
    let mut map = HeaderMap::new();
    map.insert("retry-after", value);
    let headers = Headers::from(map);
    let before = system((i128::from(at.unwrap_or(FIRST + 1)) - 1) * 1_000_000_000);
    (
        headers.retry_after_at(clock),
        headers.retry_after_at(before),
    )
}

#[test]
/// Also proves C4.3.
fn c11_5_the_http_date_corpus_reads_alike_in_both_readers() {
    let rt = runtime();
    for (line, fields) in corpus::cases(include_str!("corpus/http_date.txt")) {
        let at: Option<i64> = corpus::text(&fields[0])
            .strip_prefix("at=")
            .map(|at| at.parse().unwrap());
        // The clock that a two-digit year is read on: the line's own, or 2030.
        let clock = fields.get(2).map_or(1_893_456_000, |field| {
            corpus::text(field)
                .strip_prefix("clock=")
                .unwrap()
                .parse()
                .unwrap()
        });
        let clock = system(i128::from(clock) * 1_000_000_000);
        let text = corpus::text(&fields[1]);
        assert_eq!(http_date_model(text, clock), at, "line {line}");
        let (from_clock, from_before) = retry_after(&fields[1], at, clock);
        if at.is_some() {
            assert!(from_clock.is_some(), "line {line}");
            assert_eq!(from_before, Some(sec(1)), "line {line}");
        } else {
            assert_eq!((from_clock, from_before), (None, None), "line {line}");
        }
        // `Date` reads a two-digit year on this machine's clock: the model says what on it.
        let here = http_date_model(text, SystemTime::now());
        let until = date_reader(&fields[1], here, &rt);
        if let Some(at) = here {
            // Counted from the date: one second, or none past the year 9999.
            assert_eq!(
                until,
                Some(sec(((at + 1).min(LAST) - at).max(0) as u64)),
                "line {line}"
            );
        } else {
            // Counted from this clock: a thousand seconds, less the time it took.
            let until = until.unwrap();
            assert!(
                until <= sec(1000) && until > sec(990),
                "line {line}: {until:?}"
            );
        }
    }
}

#[test]
fn c11_5_an_rfc_850_date_takes_its_century_at_the_fifty_year_edge_of_this_clock() {
    let rt = runtime();
    for near in [-1i64, 0, 1] {
        // The response must be seen in the second its date was written for: try again if not.
        let mut checked = false;
        for _ in 0..20 {
            let start = nanos_of(SystemTime::now()).div_euclid(1_000_000_000) as i64;
            let (y, mo, d, h, mi, s) = calendar(start);
            let day = d.min(month_days(y + 50, mo));
            let edge = days_from_civil(y + 50, mo, day) * 86_400 + h * 3600 + mi * 60 + s;
            let text = http_text(edge + near, 1, 0, false);
            let mut headers = HeaderMap::new();
            header(&mut headers, "date", text.as_bytes());
            let probe = text.clone();
            // A reset one second after the date read fifty years ahead.
            let reset = rfc3339_text(i128::from(edge + near + 1) * 1_000_000_000);
            let (now, model, until) = seen(&rt, headers, move |seen| {
                let now = seen.now();
                (
                    now,
                    http_date_model(&probe, now),
                    seen.until_rfc3339(&reset),
                )
            });
            if nanos_of(now).div_euclid(1_000_000_000) as i64 != start {
                continue;
            }
            if near <= 0 {
                // Fifty years ahead or less: this century.
                assert_eq!(model, Some(edge + near), "{text}");
                assert_eq!(until, Some(sec(1)), "{text}");
            } else {
                // One second further: a hundred years back, where the weekday is not this
                // one, so it is no date; from this clock, the reset is over fifty years ahead.
                assert_eq!(model, None, "{text}");
                assert_eq!(until, None, "{text}");
            }
            checked = true;
            break;
        }
        assert!(checked, "the clock never stayed in one second");
    }
}

#[test]
fn c11_5_a_date_of_4097_bytes_is_read_by_retry_after_and_not_by_date() {
    let rt = runtime();
    let date = "Sun, 06 Nov 1994 08:49:37 GMT";
    for (width, read_by_date) in [(4096, true), (4097, false)] {
        let text = format!("{date}{}", " ".repeat(width - date.len()));
        assert_eq!(text.len(), width);
        let (_, from_before) = retry_after(text.as_bytes(), Some(784_111_777), SystemTime::now());
        assert_eq!(from_before, Some(sec(1)));
        // Read, the reset is a second after the date; not read, it is long past on this clock.
        let until = date_reader(text.as_bytes(), Some(784_111_777), &rt);
        assert_eq!(
            until,
            Some(if read_by_date { sec(1) } else { Duration::ZERO })
        );
    }
}

#[test]
fn c11_5_the_rfc3339_corpus_reads_as_written() {
    let rt = runtime();
    for (line, fields) in corpus::cases(include_str!("corpus/rfc3339.txt")) {
        let expected: Option<i128> = corpus::text(&fields[0]).strip_prefix("at=").map(|at| {
            let (seconds, nanos) = at.split_once('.').unwrap();
            let seconds: i128 = seconds.parse().unwrap();
            seconds * 1_000_000_000 + nanos.parse::<i128>().unwrap() * seconds.signum().max(1)
        });
        // Ninety seconds before the reset, as an HTTP date can write it.
        let base = expected.map_or(1_893_456_000, |at| {
            (at.div_euclid(1_000_000_000) as i64 - 90).max(FIRST)
        });
        let mut headers = HeaderMap::new();
        header(
            &mut headers,
            "date",
            http_text(base, 0, 0, false).as_bytes(),
        );
        header(&mut headers, "x-reset", &fields[1]);
        let until = seen(&rt, headers, |seen| {
            seen.until_rfc3339(seen.header("x-reset").unwrap())
        });
        assert_eq!(
            until,
            expected.map(|at| duration(at - i128::from(base) * 1_000_000_000)),
            "line {line}"
        );
        assert_eq!(
            expected,
            rfc3339_model(corpus::text(&fields[1])),
            "line {line}"
        );
    }
}

#[test]
fn c11_5_the_counts_corpus_reads_as_written() {
    let rt = runtime();
    for (line, fields) in corpus::cases(include_str!("corpus/counts.txt")) {
        let expected: usize = corpus::text(&fields[0]).parse().unwrap();
        let mut headers = HeaderMap::new();
        header(&mut headers, "x-limit", &fields[1]);
        header(&mut headers, "x-count", &fields[2]);
        let learned = seen(&rt, headers, |seen| {
            counts("x-limit", "x-count")(seen).len()
        });
        assert_eq!(learned, expected, "line {line}");
        assert_eq!(
            counts_model(corpus::text(&fields[1]), corpus::text(&fields[2])).len(),
            expected,
            "line {line}"
        );
    }
}
