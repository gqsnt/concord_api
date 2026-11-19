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
    for (index, row) in include_str!("fixtures/calendar.tsv").lines().enumerate() {
        if row.starts_with('#') {
            continue;
        }
        let mut columns = row.split('\t');
        let kind = columns.next().unwrap();
        let mut text = columns.next().unwrap().replace("\\t", "\t");
        let pad = columns.next().unwrap();
        if pad != "-" {
            let length = pad.parse::<usize>().unwrap();
            text.push_str(&" ".repeat(length - text.len()));
        }
        let context = || format!("row {}: {:?}", index + 1, text.get(..text.len().min(60)));
        let argument = columns.next().unwrap();
        let result = columns.next().unwrap();
        let parse = |text: &str| (text != "-").then(|| text.parse::<i64>().unwrap());
        match kind {
            "http" => {
                let reset = if argument == "-" {
                    rfc3339(now_seconds() + AHEAD)
                } else {
                    argument.to_owned()
                };
                let got = read(text.clone(), reset).await;
                match parse(result) {
                    Some(instant) => {
                        let ahead = instant - now_seconds();
                        let wait = got.core.unwrap_or_else(|| panic!("{}", context()));
                        if ahead > 10 {
                            let wait = i64::try_from(wait.as_secs()).unwrap();
                            assert!((ahead - 5..=ahead).contains(&wait), "{}", context());
                        } else {
                            assert_eq!(wait, Duration::ZERO, "{}", context());
                        }
                    }
                    None => assert_eq!(got.core, None, "{}", context()),
                }
                if argument == "-" {
                    assert!(fell_back(&got), "{}: {:?}", context(), got.limit);
                } else {
                    assert_eq!(got.limit, Some(Duration::from_secs(2)), "{}", context());
                }
            }
            "reset" => {
                script.answer(429).header("retry-after", &text);
                let got = core.search().await.unwrap_err().retry_after();
                let expected = columns
                    .next()
                    .map(|seconds| Duration::from_secs(seconds.parse().unwrap()));
                assert_eq!(got, expected, "{}", context());

                script
                    .answer(200)
                    .header("date", argument)
                    .header("x-reset", &text);
                extras.search().await.unwrap();
                let got = seen.lock().unwrap().last().copied().flatten();
                let expected =
                    parse(result).map(|nanos| Duration::from_nanos(nanos.try_into().unwrap()));
                assert_eq!(got, expected, "{}", context());
            }
            _ => panic!("unknown calendar case: {row}"),
        }
    }
}
