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

/// A date, and when both readers accept it, the reset two seconds after it with its instant.
type Row = (&'static str, Option<(&'static str, i64)>);

const TABLE: &[Row] = &[
    // The three forms.
    (
        "Sun, 06 Nov 1994 08:49:37 GMT",
        Some(("1994-11-06T08:49:39Z", 784_111_777)),
    ),
    (
        "Sunday, 06-Nov-94 08:49:37 GMT",
        Some(("1994-11-06T08:49:39Z", 784_111_777)),
    ),
    (
        "Sun Nov  6 08:49:37 1994",
        Some(("1994-11-06T08:49:39Z", 784_111_777)),
    ),
    (
        "Fri Dec 31 23:59:59 1999",
        Some(("2000-01-01T00:00:01Z", 946_684_799)),
    ),
    // Two-digit years, by the fifty-year rule: one behind and one ahead of this year.
    (
        "Friday, 31-Dec-99 23:59:59 GMT",
        Some(("2000-01-01T00:00:01Z", 946_684_799)),
    ),
    (
        "Tuesday, 01-Jan-30 00:00:00 GMT",
        Some(("2030-01-01T00:00:02Z", 1_893_456_000)),
    ),
    // A leap second, at the end of a month and on another day: the next second.
    (
        "Sat, 31 Dec 2016 23:59:60 GMT",
        Some(("2017-01-01T00:00:02Z", 1_483_228_800)),
    ),
    (
        "Fri, 30 Dec 2016 23:59:60 GMT",
        Some(("2016-12-31T00:00:02Z", 1_483_142_400)),
    ),
    // Years 1 and 9999, before 1970, and in the future.
    (
        "Mon, 01 Jan 0001 00:00:00 GMT",
        Some(("0001-01-01T00:00:02Z", -62_135_596_800)),
    ),
    (
        "Fri, 31 Dec 9999 23:59:57 GMT",
        Some(("9999-12-31T23:59:59Z", 253_402_300_797)),
    ),
    (
        "Wed, 31 Dec 1969 23:59:57 GMT",
        Some(("1969-12-31T23:59:59Z", -3)),
    ),
    (
        "Fri, 01 Jan 2100 00:00:00 GMT",
        Some(("2100-01-01T00:00:02Z", 4_102_444_800)),
    ),
    // A wrong weekday, in each form.
    ("Mon, 06 Nov 1994 08:49:37 GMT", None),
    ("Saturday, 31-Dec-99 23:59:59 GMT", None),
    ("Mon Nov  6 08:49:37 1994", None),
    // Shapes that are not an HTTP date.
    ("Sun, 06 Nov 1994 08:49:37 UTC", None),
    ("Sun, 6 Nov 1994 08:49:37 GMT", None),
    ("Sun, 06 nov 1994 08:49:37 GMT", None),
    ("Sun, 06 Nov 94 08:49:37 GMT", None),
    ("Sun Nov 6 08:49:37 1994", None),
    ("Sun, 06-Nov-94 08:49:37 GMT", None),
    ("Sunday, 06-Nov-1994 08:49:37 GMT", None),
    ("Sun, 06 Nov 1994 08:49:37 GMT x", None),
    ("yesterday", None),
    // Dates and times that do not exist.
    ("Sat, 01 Jan 0000 00:00:00 GMT", None),
    ("Sun, 29 Feb 2026 00:00:00 GMT", None),
    ("Sun, 06 Nov 1994 24:00:00 GMT", None),
    ("Sun, 06 Nov 1994 08:60:00 GMT", None),
    ("Sat, 31 Dec 2016 23:58:60 GMT", None),
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
    for (date, accepted) in TABLE {
        // The core: a 429 whose Retry-After is this date.
        script.answer(429).header("retry-after", date);
        let error = core.search().await.unwrap_err();
        let by_core = error.retry_after();
        // The limit: a response whose Date is this date, and a reset two seconds later.
        let reset = accepted.map_or("1994-11-06T08:49:39Z", |(reset, _)| reset);
        script
            .answer(200)
            .header("date", date)
            .header("x-reset", reset);
        extras.search().await.unwrap();
        let by_limit = seen.lock().unwrap().last().copied().flatten();
        match accepted {
            Some((_, instant)) => {
                assert!(by_core.is_some(), "the core refused {date:?}");
                assert_eq!(
                    by_limit,
                    Some(Duration::from_secs(2)),
                    "the limit refused {date:?}"
                );
                // A date in the future: the core's wait reaches the same instant.
                let ahead = instant - now_seconds();
                if ahead > 10 {
                    let wait = i64::try_from(by_core.unwrap().as_secs()).unwrap();
                    assert!(
                        (ahead - 5..=ahead).contains(&wait),
                        "{date:?}: {wait} against {ahead}"
                    );
                } else {
                    assert_eq!(by_core, Some(Duration::ZERO), "{date:?}");
                }
            }
            None => {
                assert_eq!(by_core, None, "the core accepted {date:?}");
                // The limit counts from the local clock instead: the reset is long past.
                assert_eq!(
                    by_limit,
                    Some(Duration::ZERO),
                    "the limit accepted {date:?}"
                );
            }
        }
    }
    assert_eq!(TABLE.len(), 29);
}
