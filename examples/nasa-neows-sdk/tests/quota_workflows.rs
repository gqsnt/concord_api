//! Consumer: an application that keeps NASA's reported quota after every call of both connected
//! workflows, the quota refusal included, and stops on its own when the last report says zero.
//! Offline; the counter below is a synthetic stand-in for api.nasa.gov's per-key accounting.
use std::sync::{Arc, Mutex};

use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_nasa_neows::{
    Asteroid, Error, Metered, NeoWs, NeoWsConfig, RateLimit, Reported, SendMetered,
};

// The fixtures echo this key in links and in the error body.
const KEY: &str = "synthetic-secret";
const LIMIT: u64 = 30;

fn client(server: &TestServer) -> NeoWs {
    NeoWs::new(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap(),
        NeoWsConfig {
            api_key: KEY.into(),
        },
    )
    .with_base_url(format!("{}/neo/rest/v1", server.url()))
    .unwrap()
}

/// Each answered request spends one unit; at zero the key is refused with the counter at zero.
/// Lookups of `2000433` carry no quota headers when `strip_lookup` is set (an intermediary that
/// drops them).
async fn provider(remaining: u64, strip_lookup: bool) -> TestServer {
    let counter = Arc::new(Mutex::new(remaining));
    TestServer::start(move |r: &Recorded| {
        let left = {
            let mut counter = counter.lock().unwrap();
            let left = counter.checked_sub(1);
            *counter = left.unwrap_or(0);
            left
        };
        let (reply, left) = match left {
            None => (
                Reply::json(429, include_str!("../fixtures/key-error.json"))
                    .header("x-provider-diagnostic", format!("header-echo {KEY}")),
                0,
            ),
            Some(left) => {
                let body = match r.path() {
                    "/neo/rest/v1/feed" => include_str!("../fixtures/feed.json"),
                    "/neo/rest/v1/neo/browse" => include_str!("../fixtures/browse.json"),
                    "/neo/rest/v1/neo/3542519" => include_str!("../fixtures/lookup-feed.json"),
                    "/neo/rest/v1/neo/2000433" if strip_lookup => {
                        return Reply::json(200, include_str!("../fixtures/lookup-browse.json"));
                    }
                    "/neo/rest/v1/neo/2000433" => include_str!("../fixtures/lookup-browse.json"),
                    _ => return Reply::new(404),
                };
                (Reply::json(200, body), left)
            }
        };
        reply
            .header("x-ratelimit-limit", LIMIT.to_string())
            .header("x-ratelimit-remaining", left.to_string())
    })
    .await
}

#[derive(Debug, PartialEq)]
enum Outcome {
    Ok,
    Failed(ErrorKind, Option<u16>),
}

/// The application's own record: what NASA reported after each call.
#[derive(Debug, Default)]
struct QuotaLog {
    calls: Vec<(&'static str, Outcome, RateLimit)>,
    skipped: Vec<&'static str>,
}

impl QuotaLog {
    fn record<T>(&mut self, call: &'static str, result: Result<Metered<T>, Error>) -> Option<T> {
        match result {
            Ok(metered) => {
                self.calls.push((call, Outcome::Ok, metered.rate_limit));
                Some(metered.value)
            }
            Err(error) => {
                let outcome = Outcome::Failed(error.kind, error.status.map(|s| s.as_u16()));
                self.calls.push((call, outcome, error.rate_limit));
                None
            }
        }
    }

    /// Exhausted only on a reported zero; an absent or unparsable report is unknown, not zero.
    fn may_send(&mut self, careful: bool, call: &'static str) -> bool {
        let last = self.calls.last().map(|(_, _, quota)| quota.remaining);
        let allowed = !careful || last != Some(Reported::Value(0));
        if !allowed {
            self.skipped.push(call);
        }
        allowed
    }

    fn remaining(&self) -> Vec<Reported> {
        self.calls
            .iter()
            .map(|(_, _, quota)| quota.remaining)
            .collect()
    }
}

async fn feed_then_lookup(api: &NeoWs, log: &mut QuotaLog, careful: bool) -> Vec<Asteroid> {
    let neo = api.neows();
    let mut details = Vec::new();
    if !log.may_send(careful, "feed") {
        return details;
    }
    let sent = neo.feed("2015-09-07", "2015-09-08").send_metered().await;
    let Some(feed) = log.record("feed", sent) else {
        return details;
    };
    for object in feed.near_earth_objects.values().flatten() {
        if !log.may_send(careful, "lookup") {
            break;
        }
        match log.record("lookup", neo.lookup(&object.id).send_metered().await) {
            Some(detail) => details.push(detail),
            None => break,
        }
    }
    details
}

async fn browse_then_inspect(api: &NeoWs, log: &mut QuotaLog, careful: bool) -> Vec<Asteroid> {
    let neo = api.neows();
    let mut details = Vec::new();
    if !log.may_send(careful, "browse") {
        return details;
    }
    let sent = neo.browse().set_page(0).set_size(1).send_metered().await;
    let Some(page) = log.record("browse", sent) else {
        return details;
    };
    for object in &page.near_earth_objects {
        if !log.may_send(careful, "lookup") {
            break;
        }
        match log.record("lookup", neo.lookup(&object.id).send_metered().await) {
            Some(detail) => details.push(detail),
            None => break,
        }
    }
    details
}

fn assert_log_safe(log: &QuotaLog) {
    let rendered = format!("{log:?} {log:#?}");
    for forbidden in [KEY, "header-echo", "API_KEY_INVALID", "http"] {
        assert!(!rendered.contains(forbidden), "unsafe log: {rendered}");
    }
}

#[tokio::test]
async fn quota_is_kept_after_every_call_including_the_refusal() {
    let server = provider(3, false).await;
    let api = client(&server);
    let mut log = QuotaLog::default();
    let from_feed = feed_then_lookup(&api, &mut log, false).await;
    let from_browse = browse_then_inspect(&api, &mut log, false).await;
    assert_eq!(from_feed.len(), 1);
    assert_eq!(from_feed[0].name, "Synthetic Alpha");
    assert!(from_browse.is_empty(), "the inspection was refused");
    let quota = |remaining| RateLimit {
        limit: Reported::Value(LIMIT),
        remaining: Reported::Value(remaining),
    };
    assert_eq!(
        log.calls,
        [
            ("feed", Outcome::Ok, quota(2)),
            ("lookup", Outcome::Ok, quota(1)),
            ("browse", Outcome::Ok, quota(0)),
            (
                "lookup",
                Outcome::Failed(ErrorKind::Status, Some(429)),
                quota(0)
            ),
        ]
    );
    assert!(log.skipped.is_empty());
    assert_log_safe(&log);
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn a_careful_caller_stops_on_a_reported_zero_and_not_on_an_unknown() {
    // A reported zero: the inspection is not sent.
    let server = provider(3, false).await;
    let api = client(&server);
    let mut log = QuotaLog::default();
    let from_feed = feed_then_lookup(&api, &mut log, true).await;
    let from_browse = browse_then_inspect(&api, &mut log, true).await;
    assert_eq!((from_feed.len(), from_browse.len()), (1, 0));
    assert_eq!(
        log.remaining(),
        [Reported::Value(2), Reported::Value(1), Reported::Value(0)]
    );
    assert_eq!(log.skipped, ["lookup"]);
    assert_eq!(
        server.requests().len(),
        3,
        "no request after a reported zero"
    );
    assert_log_safe(&log);
    server.shutdown().await;

    // Browse first: its inspection's quota headers are stripped, so the next call is sent and
    // NASA's own refusal reports the zero.
    let server = provider(2, true).await;
    let api = client(&server);
    let mut log = QuotaLog::default();
    let from_browse = browse_then_inspect(&api, &mut log, true).await;
    let from_feed = feed_then_lookup(&api, &mut log, true).await;
    assert_eq!(from_browse.len(), 1);
    assert_eq!(from_browse[0].id, "2000433");
    assert!(from_feed.is_empty());
    assert_eq!(
        log.remaining(),
        [Reported::Value(1), Reported::Absent, Reported::Value(0)]
    );
    assert_eq!(
        log.calls[2].1,
        Outcome::Failed(ErrorKind::Status, Some(429))
    );
    assert!(log.skipped.is_empty());
    assert_eq!(server.requests().len(), 3);
    assert_log_safe(&log);
    server.shutdown().await;
}
