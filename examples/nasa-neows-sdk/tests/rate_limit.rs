//! Regression: every NeoWs result, success or failure, reports the `X-RateLimit-*` quota that
//! api.nasa.gov documents on every response, with a distinct absent and unparsable outcome, and
//! never exposes the API key. Offline; the headers and bodies are synthetic.
use concord::{ErrorKind, reqwest};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_nasa_neows::{
    BODY_LIMIT, BrowsePage, Error, Feed, NeoWs, NeoWsConfig, RateLimit, Reported, SendMetered,
};

const KEY: &str = "quota-secret-key";

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

fn quota(limit: u64, remaining: u64) -> RateLimit {
    RateLimit {
        limit: Reported::Value(limit),
        remaining: Reported::Value(remaining),
    }
}

fn with_quota(reply: Reply, limit: &str, remaining: &str) -> Reply {
    reply
        .header("x-ratelimit-limit", limit)
        .header("x-ratelimit-remaining", remaining)
}

fn assert_no_key(rendered: &str) {
    assert!(!rendered.contains(KEY), "key in {rendered}");
    assert!(
        !rendered.contains("quota-secret"),
        "key prefix in {rendered}"
    );
    assert!(
        !rendered.contains("header-echo"),
        "header text in {rendered}"
    );
}

fn assert_safe(error: &Error) {
    for rendered in [
        format!("{error}"),
        format!("{error:?}"),
        format!("{error:#?}"),
    ] {
        assert_no_key(&rendered);
    }
    assert!(std::error::Error::source(error).is_none());
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Feed,
    Browse,
    Lookup,
}
const OPS: [Op; 3] = [Op::Feed, Op::Browse, Op::Lookup];

impl Op {
    fn path(self) -> &'static str {
        match self {
            Op::Feed => "/neo/rest/v1/feed",
            Op::Browse => "/neo/rest/v1/neo/browse",
            Op::Lookup => "/neo/rest/v1/neo/3542519",
        }
    }

    fn fixture(self) -> &'static str {
        match self {
            Op::Feed => include_str!("../fixtures/feed.json"),
            Op::Browse => include_str!("../fixtures/browse.json"),
            Op::Lookup => include_str!("../fixtures/lookup-feed.json"),
        }
    }

    // Through the generated `send()`: the value only, the quota on failure.
    async fn send(self, api: &NeoWs) -> Result<(), Error> {
        let neo = api.neows();
        match self {
            Op::Feed => neo.feed("2015-09-07", "2015-09-08").send().await.map(drop),
            Op::Browse => neo.browse().set_page(0).set_size(1).send().await.map(drop),
            Op::Lookup => neo.lookup("3542519").send().await.map(drop),
        }
    }

    async fn send_metered(self, api: &NeoWs) -> Result<RateLimit, Error> {
        let neo = api.neows();
        let rendered;
        let rate_limit = match self {
            Op::Feed => {
                let metered = neo.feed("2015-09-07", "2015-09-08").send_metered().await?;
                rendered = format!("{metered:?}");
                metered.rate_limit
            }
            Op::Browse => {
                let metered = neo.browse().set_page(0).set_size(1).send_metered().await?;
                rendered = format!("{metered:?}");
                metered.rate_limit
            }
            Op::Lookup => {
                let metered = neo.lookup("3542519").send_metered().await?;
                rendered = format!("{metered:?}");
                metered.rate_limit
            }
        };
        assert_no_key(&rendered);
        Ok(rate_limit)
    }
}

#[tokio::test]
async fn success_results_report_quota_for_every_endpoint() {
    let server = TestServer::start(|r: &Recorded| {
        let remaining = match r.path() {
            "/neo/rest/v1/feed" => "999",
            "/neo/rest/v1/neo/browse" => "998",
            _ => "997",
        };
        let body = match r.path() {
            "/neo/rest/v1/feed" => Op::Feed.fixture(),
            "/neo/rest/v1/neo/browse" => Op::Browse.fixture(),
            _ => Op::Lookup.fixture(),
        };
        with_quota(Reply::json(200, body), "1000", remaining)
    })
    .await;
    let api = client(&server);
    let neo = api.neows();

    let feed = neo
        .feed("2015-09-07", "2015-09-08")
        .send_metered()
        .await
        .unwrap();
    assert_eq!(feed.value.element_count, 1);
    assert_eq!(feed.rate_limit, quota(1000, 999));
    let page = neo
        .browse()
        .set_page(0)
        .set_size(1)
        .send_metered()
        .await
        .unwrap();
    assert_eq!(page.value.page.total_pages, 2);
    assert_eq!(page.rate_limit, quota(1000, 998));
    let detail = neo.lookup("3542519").send_metered().await.unwrap();
    assert_eq!(detail.value.id, "3542519");
    assert_eq!(detail.rate_limit, quota(1000, 997));
    assert_eq!(detail.rate_limit.remaining.value(), Some(997));
    for rendered in [
        format!("{feed:?}"),
        format!("{page:?}"),
        format!("{detail:?}"),
    ] {
        assert_no_key(&rendered);
    }

    // `send()` keeps returning the bare value.
    let _: Feed = neo.feed("2015-09-07", "2015-09-08").send().await.unwrap();
    let _: BrowsePage = neo.browse().send().await.unwrap();
    assert_eq!(neo.lookup("3542519").send().await.unwrap(), detail.value);

    // One request per call either way; the key travels only in its query parameter.
    let requests = server.requests();
    assert_eq!(requests.len(), 6);
    for (request, op) in requests.iter().zip(OPS.iter().chain(OPS.iter())) {
        assert_eq!(request.path(), op.path());
        assert_eq!(request.query_all("api_key"), [KEY]);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn absent_and_unparsable_headers_are_distinguished() {
    let echo = format!("header-echo {KEY}");
    let cases: Vec<(Vec<String>, Vec<String>, Reported, Reported)> = [
        (vec![], vec![], Reported::Absent, Reported::Absent),
        (
            vec!["1000"],
            vec![],
            Reported::Value(1000),
            Reported::Absent,
        ),
        (vec![], vec!["0"], Reported::Absent, Reported::Value(0)),
        (
            vec!["abc"],
            vec!["7"],
            Reported::Unparsable,
            Reported::Value(7),
        ),
        (
            vec![""],
            vec![" 7 "],
            Reported::Unparsable,
            Reported::Value(7),
        ),
        (
            vec!["-1"],
            vec!["+5"],
            Reported::Unparsable,
            Reported::Unparsable,
        ),
        (
            vec!["1.5"],
            vec!["1e3"],
            Reported::Unparsable,
            Reported::Unparsable,
        ),
        (
            vec!["5, 4"],
            vec!["5", "5"],
            Reported::Unparsable,
            Reported::Unparsable,
        ),
        (
            vec!["18446744073709551615"],
            vec!["18446744073709551616"],
            Reported::Value(u64::MAX),
            Reported::Unparsable,
        ),
        (
            vec!["\u{663}"],
            vec![echo.as_str()],
            Reported::Unparsable,
            Reported::Unparsable,
        ),
    ]
    .into_iter()
    .map(|(limit, remaining, l, r)| {
        let owned = |v: Vec<&str>| v.into_iter().map(str::to_owned).collect();
        (owned(limit), owned(remaining), l, r)
    })
    .collect();

    for (limit, remaining, expected_limit, expected_remaining) in cases {
        let expected = RateLimit {
            limit: expected_limit,
            remaining: expected_remaining,
        };
        let headers = (limit.clone(), remaining.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let mut reply = match r.path() {
                "/neo/rest/v1/feed" => Reply::json(200, Op::Feed.fixture()),
                _ => Reply::json(429, include_str!("../fixtures/key-error.json")),
            };
            for value in &headers.0 {
                reply = reply.header("x-ratelimit-limit", value.as_str());
            }
            for value in &headers.1 {
                reply = reply.header("x-ratelimit-remaining", value.as_str());
            }
            reply
        })
        .await;
        let api = client(&server);
        let case = format!("limit {limit:?} remaining {remaining:?}");
        // The same reading on a success and on the quota refusal itself.
        let success = Op::Feed.send_metered(&api).await.expect(&case);
        assert_eq!(success, expected, "{case}");
        let error = Op::Lookup.send(&api).await.unwrap_err();
        assert_eq!(error.kind, ErrorKind::Status, "{case}");
        assert_eq!(error.status.map(|s| s.as_u16()), Some(429), "{case}");
        assert_eq!(error.rate_limit, expected, "{case}");
        assert_safe(&error);
        assert_eq!(server.requests().len(), 2, "{case}");
        server.shutdown().await;
    }
}

#[tokio::test]
async fn failures_report_quota_at_every_stage_after_the_head() {
    for op in OPS {
        for (case, kind, status) in [
            ("invalid_key", ErrorKind::Status, 403),
            ("over_limit_stalled_body", ErrorKind::Status, 429),
            ("malformed", ErrorKind::Decode, 200),
            ("wrong_content_type", ErrorKind::Metadata, 200),
            ("oversized", ErrorKind::Limit, 200),
            ("truncated", ErrorKind::Transport, 200),
        ] {
            let server = TestServer::start(move |_: &Recorded| {
                let reply = match case {
                    "invalid_key" => Reply::json(403, include_str!("../fixtures/key-error.json")),
                    "over_limit_stalled_body" => Reply::new(429)
                        .header("content-type", "application/json")
                        .with_body(Body::Stall),
                    "malformed" => Reply::json(200, include_str!("../fixtures/malformed.json")),
                    "wrong_content_type" => Reply::new(200)
                        .header("content-type", "text/html")
                        .with_body(Body::Stall),
                    "oversized" => Reply::json(200, vec![b' '; BODY_LIMIT + 1]),
                    "truncated" => Reply::new(200)
                        .header("content-type", "application/json")
                        .with_body(Body::FailAfter(vec![b"{\"id\":".as_slice().into()])),
                    _ => unreachable!(),
                };
                with_quota(reply, "30", "0")
            })
            .await;
            let api = client(&server);
            let context = format!("{op:?} {case}");
            let plain = op.send(&api).await.unwrap_err();
            let metered = op.send_metered(&api).await.unwrap_err();
            for error in [&plain, &metered] {
                assert_eq!(error.kind, kind, "{context}");
                assert_eq!(error.status.map(|s| s.as_u16()), Some(status), "{context}");
                assert_eq!(error.rate_limit, quota(30, 0), "{context}");
                assert_safe(error);
            }
            assert_eq!(plain, metered, "{context}");
            assert_eq!(
                server.requests().len(),
                2,
                "{context}: one request per call"
            );
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn no_response_reports_absent_quota() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let api = client(&server);
    let build = api.neows().lookup(".").send_metered().await.unwrap_err();
    assert_eq!(build.kind, ErrorKind::Build);
    assert!(server.requests().is_empty());
    for op in OPS {
        let plain = op.send(&api).await.unwrap_err();
        let metered = op.send_metered(&api).await.unwrap_err();
        for error in [&build, &plain, &metered] {
            assert_eq!(error.status, None, "{op:?}");
            assert_eq!(error.rate_limit, RateLimit::default(), "{op:?}");
            assert_eq!(error.rate_limit.remaining, Reported::Absent, "{op:?}");
            assert_eq!(error.rate_limit.remaining.value(), None, "{op:?}");
            assert_safe(error);
        }
        assert_eq!(plain.kind, ErrorKind::Transport, "{op:?}");
        assert_eq!(metered.kind, ErrorKind::Transport, "{op:?}");
    }
    server.shutdown().await;
}

// The public reader for a response obtained natively (`prepare()` then reqwest), where nothing
// has normalized the field: surrounding whitespace is not part of the value; other bytes are.
#[test]
fn native_header_maps_are_read_the_same_way() {
    use reqwest::header::{HeaderMap, HeaderValue};
    let read = |limit: &[u8], remaining: &[u8]| {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-limit", HeaderValue::from_bytes(limit).unwrap());
        headers.insert(
            "X-RateLimit-Remaining",
            HeaderValue::from_bytes(remaining).unwrap(),
        );
        RateLimit::from_headers(&headers)
    };
    assert_eq!(read(b" 1000\t", b"\t42 "), quota(1000, 42));
    let unparsable = RateLimit {
        limit: Reported::Unparsable,
        remaining: Reported::Unparsable,
    };
    assert_eq!(read(b"1 000", b"\xff"), unparsable);
    assert_eq!(read(b"0x10", b"4\xc2\xa02"), unparsable);
    assert_eq!(
        RateLimit::from_headers(&HeaderMap::new()),
        RateLimit::default()
    );
}
