//! C4.3 and C11.5: the two readers of an HTTP date agree.
//!
//! The core reads a date in `Retry-After`; the limit reads one in `Date`, to count an RFC 3339
//! reset from the server's clock. They are two readers of one grammar (RFC 9110, section
//! 5.6.7), so one table of dates runs through both, each through public behaviour: the core's
//! as the `retry_after()` of a refused call, the limit's as `Seen::until_rfc3339`.

use concord::limit::{Bucket, Limit, Seen};
use concord_testkit::Script;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

concord::api! {
    client Shop {
        base "https://shop.example.test/api";
        read concord::Text;
        GET search() -> String { path "search"; }
    }
}

/// What both readers make of a date.
#[derive(Clone, Copy)]
enum Outcome {
    /// Both accept it: the reset written two seconds after it, and its instant in Unix seconds.
    Accepted(&'static str, i64),
    /// Both refuse it.
    Refused,
}
use Outcome::{Accepted, Refused};

const TABLE: &[(&str, Outcome)] = &[
    // The three forms.
    (
        "Sun, 06 Nov 1994 08:49:37 GMT",
        Accepted("1994-11-06T08:49:39Z", 784_111_777),
    ),
    (
        "Sunday, 06-Nov-94 08:49:37 GMT",
        Accepted("1994-11-06T08:49:39Z", 784_111_777),
    ),
    (
        "Sun Nov  6 08:49:37 1994",
        Accepted("1994-11-06T08:49:39Z", 784_111_777),
    ),
    (
        "Fri Dec 31 23:59:59 1999",
        Accepted("2000-01-01T00:00:01Z", 946_684_799),
    ),
    // Two-digit years, by the fifty-year rule: one behind and one ahead of this year.
    (
        "Friday, 31-Dec-99 23:59:59 GMT",
        Accepted("2000-01-01T00:00:01Z", 946_684_799),
    ),
    (
        "Tuesday, 01-Jan-30 00:00:00 GMT",
        Accepted("2030-01-01T00:00:02Z", 1_893_456_000),
    ),
    // A leap second is the next second. The grammar allows it on any day at 23:59:60,
    // also on a day when UTC had none (29 June 2015).
    (
        "Sat, 31 Dec 2016 23:59:60 GMT",
        Accepted("2017-01-01T00:00:02Z", 1_483_228_800),
    ),
    (
        "Fri, 30 Dec 2016 23:59:60 GMT",
        Accepted("2016-12-31T00:00:02Z", 1_483_142_400),
    ),
    (
        "Mon, 29 Jun 2015 23:59:60 GMT",
        Accepted("2015-06-30T00:00:02Z", 1_435_622_400),
    ),
    // Years 1 and 9999, before 1970, and in the future.
    (
        "Mon, 01 Jan 0001 00:00:00 GMT",
        Accepted("0001-01-01T00:00:02Z", -62_135_596_800),
    ),
    (
        "Fri, 31 Dec 9999 23:59:57 GMT",
        Accepted("9999-12-31T23:59:59Z", 253_402_300_797),
    ),
    (
        "Wed, 31 Dec 1969 23:59:57 GMT",
        Accepted("1969-12-31T23:59:59Z", -3),
    ),
    (
        "Fri, 01 Jan 2100 00:00:00 GMT",
        Accepted("2100-01-01T00:00:02Z", 4_102_444_800),
    ),
    // A wrong weekday, in each form.
    ("Mon, 06 Nov 1994 08:49:37 GMT", Refused),
    ("Saturday, 31-Dec-99 23:59:59 GMT", Refused),
    ("Mon Nov  6 08:49:37 1994", Refused),
    // Shapes that are not an HTTP date.
    ("Sun, 06 Nov 1994 08:49:37 UTC", Refused),
    ("Sun, 6 Nov 1994 08:49:37 GMT", Refused),
    ("Sun, 06 nov 1994 08:49:37 GMT", Refused),
    ("Sun, 06 Nov 94 08:49:37 GMT", Refused),
    ("Sun Nov 6 08:49:37 1994", Refused),
    ("Sun, 06-Nov-94 08:49:37 GMT", Refused),
    ("Sunday, 06-Nov-1994 08:49:37 GMT", Refused),
    ("Sun, 06 Nov 1994 08:49:37 GMT x", Refused),
    ("yesterday", Refused),
    // Dates and times that do not exist.
    ("Sat, 01 Jan 0000 00:00:00 GMT", Refused),
    ("Sun, 29 Feb 2026 00:00:00 GMT", Refused),
    ("Sun, 06 Nov 1994 24:00:00 GMT", Refused),
    ("Sun, 06 Nov 1994 08:60:00 GMT", Refused),
    ("Sat, 31 Dec 2016 23:58:60 GMT", Refused),
];

fn now_seconds() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

/// An RFC 3339 date-time for Unix seconds after 1970.
fn rfc3339(seconds: i64) -> String {
    let seconds = u64::try_from(seconds).unwrap();
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let (h, m, s) = (rest / 3600, rest % 3600 / 60, rest % 60);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// What the core and the limit make of one `date`.
struct Read {
    core: Option<Duration>,
    limit: Option<Duration>,
}

/// A refused date leaves the limit on the local clock: then the reset, 1,000 s ahead of it,
/// is about 1,000 s away, which no accepted date of the table gives.
const AHEAD: i64 = 1000;

fn fell_back(read: &Read) -> bool {
    read.limit
        .is_some_and(|until| until > Duration::from_secs(990) && until <= Duration::from_secs(1000))
}

#[tokio::test(start_paused = true)]
async fn c11_5_the_reader_of_date_agrees_with_the_reader_of_retry_after() {
    let script = Script::new();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = seen.clone();
    let limit = Limit::new().bucket(Bucket::new(|_| Some("k".into())).learn(
        move |seen: &Seen<'_>| {
            let until = seen
                .header("x-reset")
                .and_then(|text| seen.until_rfc3339(text));
            kept.lock().unwrap().push(until);
            Vec::new()
        },
    ));
    let core = Shop::new().with_transport(script.clone());
    let extras = Shop::new()
        .with_transport(script.clone())
        .with_middleware(limit);
    let read = |date: String, reset: String| {
        let (script, core, extras, seen) = (&script, &core, &extras, &seen);
        async move {
            // The core: a 429 whose Retry-After is this date.
            script.answer(429).header("retry-after", &date);
            let core = core.search().await.unwrap_err().retry_after();
            // The limit: a response whose Date is this date.
            script
                .answer(200)
                .header("date", &date)
                .header("x-reset", &reset);
            extras.search().await.unwrap();
            let limit = seen.lock().unwrap().last().copied().flatten();
            Read { core, limit }
        }
    };
    for (date, outcome) in TABLE {
        let local_reset = rfc3339(now_seconds() + AHEAD);
        match *outcome {
            Accepted(reset, instant) => {
                let got = read((*date).to_owned(), reset.to_owned()).await;
                assert!(got.core.is_some(), "the core refused {date:?}");
                assert_eq!(
                    got.limit,
                    Some(Duration::from_secs(2)),
                    "the limit read {date:?}"
                );
                // A date in the future: the core's wait reaches the same instant.
                let ahead = instant - now_seconds();
                let wait = got.core.unwrap();
                if ahead > 10 {
                    let wait = i64::try_from(wait.as_secs()).unwrap();
                    assert!(
                        (ahead - 5..=ahead).contains(&wait),
                        "{date:?}: {wait} against {ahead}"
                    );
                } else {
                    assert_eq!(wait, Duration::ZERO, "{date:?}");
                }
            }
            Refused => {
                let got = read((*date).to_owned(), local_reset).await;
                assert_eq!(got.core, None, "the core accepted {date:?}");
                assert!(
                    fell_back(&got),
                    "the limit accepted {date:?}: {:?}",
                    got.limit
                );
            }
        }
    }
    // The one difference, ruled: the limit reads no `Date` over 4,096 bytes; the core reads a
    // `Retry-After` of any length.
    let long = format!("Sun, 06 Nov 1994 08:49:37 GMT{}", " ".repeat(4097 - 29));
    let got = read(long, rfc3339(now_seconds() + AHEAD)).await;
    assert_eq!(got.core, Some(Duration::ZERO));
    assert!(fell_back(&got), "{:?}", got.limit);
    assert_eq!(TABLE.len(), 30);
}
