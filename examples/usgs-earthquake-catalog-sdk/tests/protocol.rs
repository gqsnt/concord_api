//! Wire-contract, malformed-body, resource-limit and redaction tests on loopback only.
mod common;

use common::*;
use concord_sdk_usgs_earthquake_catalog::{
    CSV_BODY_LIMIT, CSV_RECORD_LIMIT, ERROR_BODY_LIMIT, Error, EventId, HttpKind, Magnitude,
    PageSize, SUCCESS_BODY_LIMIT, Timestamp, Usgs, Window,
    concord::{ErrorKind, bytes::Bytes, reqwest},
    concord_codecs::stream::{Completion, ReadErrorKind},
};
use concord_testserver::{Body, Recorded, Reply, Script, TestServer, paused};
use std::{
    collections::BTreeSet,
    num::{NonZeroU64, NonZeroUsize},
    time::{Duration, UNIX_EPOCH},
};

fn read_kind(error: Error) -> ReadErrorKind {
    match error {
        Error::Stream(error) => error.kind(),
        other => panic!("{other}"),
    }
}

fn concord_kind(error: &Error) -> ErrorKind {
    match error {
        Error::Concord(error) => error.kind(),
        other => panic!("{other}"),
    }
}

#[tokio::test]
async fn header_only_csv_requires_catalog_columns_but_allows_reordering_and_additions() {
    for valid in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            csv(if valid {
                include_str!("../fixtures/reordered-header-only.csv")
            } else {
                include_str!("../fixtures/bad-header-only.csv")
            })
        })
        .await;
        let catalog = client(&server).catalog();
        let mut rows = catalog.query(&filter(3)).csv().send().await.unwrap().data;
        if valid {
            assert!(rows.next().await.is_none());
            assert_eq!(rows.completion(), Completion::EndOfInput);
        } else {
            assert!(matches!(rows.next().await, Some(Err(Error::Protocol(_)))));
            assert_eq!(rows.completion(), Completion::Failed);
            assert!(rows.next().await.is_none());
            let mut export = catalog.export(&filter(3), NonZeroU64::MIN, NonZeroUsize::MIN);
            assert!(matches!(export.next().await, Err(Error::Protocol(_))));
            assert_eq!(export.rows_delivered(), 0);
            assert_eq!(export.completion(), Completion::Failed);
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn exact_wire_shapes_share_filters_and_keep_event_lookup_separate() {
    let server = TestServer::start(|request: &Recorded| {
        if !request.query_all("eventid").is_empty() {
            Reply::json(200, EVENT)
        } else if request.query_all("format") == ["csv"] {
            csv(CSV)
        } else {
            Reply::json(200, WINDOW)
        }
    })
    .await;
    let api = client(&server);
    let query = api.catalog().query(&filter(3));
    query
        .geojson()
        .set_offset(NonZeroU64::new(7).unwrap())
        .send()
        .await
        .unwrap();
    let mut rows = query
        .csv()
        .set_offset(NonZeroU64::new(7).unwrap())
        .send()
        .await
        .unwrap()
        .data;
    while let Some(row) = rows.next().await {
        row.unwrap();
    }
    let id = EventId::new("public/id&format=csv + café").unwrap();
    api.catalog().event(id).send().await.unwrap();
    let requests = server.requests();
    for request in &requests {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/fdsnws/event/1/query");
        assert_eq!(request.body_len, 0);
        assert!(request.header("authorization").is_none());
        assert!(request.header("content-type").is_none());
        let keys: Vec<_> = request
            .query_pairs()
            .into_iter()
            .map(|(key, _)| key)
            .collect();
        assert_eq!(keys.len(), keys.iter().collect::<BTreeSet<_>>().len());
    }
    let expected = vec![
        ("nodata", "404"),
        ("starttime", "2014-01-01T00:00:00Z"),
        ("endtime", "2014-01-02T00:00:00Z"),
        ("minmagnitude", "4"),
        ("limit", "3"),
        ("orderby", "time-asc"),
        ("eventtype", "earthquake"),
        ("offset", "7"),
    ];
    for (request, format, accept) in [
        (&requests[0], "geojson", "application/json"),
        (&requests[1], "csv", "text/csv"),
    ] {
        let pairs = request.query_pairs();
        let pairs: Vec<_> = pairs
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(&pairs[..8], expected.as_slice());
        assert_eq!(pairs[8], ("format", format));
        assert_eq!(request.header("accept"), Some(accept));
    }
    assert_eq!(
        requests[2].query_pairs(),
        [
            ("nodata".into(), "404".into()),
            ("eventid".into(), "public/id&format=csv + café".into()),
            ("format".into(), "geojson".into()),
        ]
    );
    assert!(
        requests[2]
            .target
            .contains("public%2Fid%26format%3Dcsv+%2B+caf%C3%A9")
    );
    server.shutdown().await;
}

#[tokio::test]
async fn input_guards_and_scope_setters_fail_before_io() {
    for (start, end) in [
        ("not a date", "2014-01-02T00:00:00Z"),
        ("2014-02-30T00:00:00Z", "2014-03-01T00:00:00Z"),
        ("2014-01-02T00:00:00Z", "2014-01-01T00:00:00Z"),
        ("2014-01-01T00:00:00Z", "2014-02-02T00:00:00Z"),
    ] {
        assert!(matches!(
            Window::new(start, end),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(Window::new("2014-01-01T01:00:00+01:00", "2014-01-01T00:00:00Z").is_ok());
    for n in [0, 20_001, u16::MAX] {
        assert!(PageSize::new(n).is_err());
    }
    for n in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(Magnitude::new(n).is_err());
    }
    for id in ["", "bad\nsecret"] {
        assert!(EventId::new(id).is_err());
    }
    assert!(EventId::new("x".repeat(257)).is_err());
    assert!(NonZeroU64::new(0).is_none());
    Usgs::validate_header_names().unwrap();
    let server = TestServer::start(|_: &Recorded| panic!("invalid inputs must not send")).await;
    let scope = client(&server)
        .catalog()
        .query(&filter(3))
        .set_endtime(Timestamp::parse("2013-12-31T00:00:00Z").unwrap());
    assert!(matches!(
        scope.geojson().send().await,
        Err(Error::InvalidInput(_))
    ));
    assert!(matches!(
        scope.csv().send().await,
        Err(Error::InvalidInput(_))
    ));
    assert!(server.requests().is_empty());
    let error = Usgs::for_fixture(reqwest::Client::new(), "http://user:secret-base@localhost/")
        .unwrap_err();
    assert!(matches!(&error, Error::Concord(source) if source.kind() == ErrorKind::Build));
    assert!(!format!("{error:?} {error}").contains("secret-base"));
    server.shutdown().await;
}

#[tokio::test]
async fn every_http_class_is_preserved_without_retries_or_secret_output() {
    for (status, kind) in [
        (204, HttpKind::NoData),
        (404, HttpKind::NoData),
        (400, HttpKind::BadRequest),
        (401, HttpKind::Unauthorized),
        (403, HttpKind::Forbidden),
        (409, HttpKind::Deleted),
        (413, HttpKind::TooLarge),
        (414, HttpKind::UriTooLong),
        (429, HttpKind::RateLimited),
        (500, HttpKind::Server),
        (503, HttpKind::Server),
        (418, HttpKind::Unexpected),
        (302, HttpKind::Unexpected),
        (206, HttpKind::Unexpected),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            let body = if status == 204 { "" } else { ERROR_TEXT };
            Reply::text(status, "text/plain", body)
                .header("set-cookie", "secret-response-header")
                .header("location", "http://127.0.0.1:1/never-follow")
                .header("retry-after", "12")
        })
        .await;
        let api = client(&server);
        let operation = api
            .catalog()
            .event(EventId::new("secret-event-id").unwrap());
        assert!(!format!("{operation:?}").contains("secret-event-id"));
        assert!(!format!("{:?}", operation.clone().prepare().unwrap()).contains("secret-event-id"));
        let error = operation.send().await.unwrap_err();
        let shown = format!("{error:?} {error}");
        for secret in [
            "secret-provider-body",
            "secret-response-header",
            "secret-event-id",
        ] {
            assert!(!shown.contains(secret), "{shown}");
        }
        let Error::Http(failure) = error else {
            panic!("expected HTTP error")
        };
        assert_eq!(failure.kind, kind);
        assert_eq!(failure.meta.status().as_u16(), status);
        assert_eq!(
            failure.retry_after(UNIX_EPOCH).unwrap(),
            Some(Duration::from_secs(12))
        );
        if status != 204 {
            assert!(
                failure
                    .document_unredacted()
                    .unwrap()
                    .contains("secret-provider-body")
            );
        }
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn bad_error_documents_keep_status_and_class() {
    let script = Script::new([
        Reply::text(503, "text/plain", [0xff]),
        Reply::text(503, "text/plain; charset=iso-8859-1", "secret-body"),
        Reply::text(503, "text/html", "x".repeat(ERROR_BODY_LIMIT + 1)),
        Reply::text(503, "text/plain", "")
            .with_body(Body::FailAfter(vec![Bytes::from_static(b"secret-prefix")])),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    for expected in [
        ErrorKind::Decode,
        ErrorKind::Metadata,
        ErrorKind::Limit,
        ErrorKind::Transport,
    ] {
        let error = client(&server)
            .catalog()
            .query(&filter(3))
            .geojson()
            .send()
            .await
            .unwrap_err();
        assert!(!format!("{error:?} {error}").contains("secret-"));
        let Error::Http(failure) = error else {
            panic!("HTTP status lost")
        };
        assert_eq!(failure.kind, HttpKind::Server);
        assert_eq!(failure.meta.status().as_u16(), 503);
        assert_eq!(failure.document_unredacted().unwrap_err().kind(), expected);
    }
    server.shutdown().await;
}

#[tokio::test]
async fn finite_body_failures_are_typed_and_keep_the_head() {
    let script = Script::new([
        Reply::json(200, include_str!("../fixtures/malformed.geojson")),
        Reply::json(200, include_str!("../fixtures/truncated.geojson")),
        Reply::json(200, ""),
        Reply::json(200, EVENT), // wrong shape for FeatureCollection
        Reply::json(200, " ".repeat(SUCCESS_BODY_LIMIT + 1)),
        Reply::json(200, "").with_body(Body::FailAfter(vec![Bytes::from_static(b"{")])),
    ]);
    let server = TestServer::start(move |_: &Recorded| script.next()).await;
    for expected in [
        ErrorKind::Decode,
        ErrorKind::Decode,
        ErrorKind::Decode,
        ErrorKind::Decode,
        ErrorKind::Limit,
        ErrorKind::Transport,
    ] {
        let error = client(&server)
            .catalog()
            .query(&filter(3))
            .geojson()
            .send()
            .await
            .unwrap_err();
        assert_eq!(concord_kind(&error), expected);
        assert!(!format!("{error:?} {error}").contains("secret-provider-body"));
        let Error::Concord(inner) = error else {
            unreachable!()
        };
        assert_eq!(inner.head().unwrap().status().as_u16(), 200);
        assert_eq!(inner.endpoint(), Some("Usgs.catalog.window.geojson"));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn both_media_policies_reject_before_a_stalled_body_is_read() {
    for is_csv in [false, true] {
        let valid = if is_csv {
            "text/csv"
        } else {
            "application/json"
        };
        let script = Script::new([
            Reply::new(200).with_body(Body::Stall),
            Reply::text(200, "text/html", "").with_body(Body::Stall),
            Reply::text(200, valid, "")
                .header("content-type", valid)
                .with_body(Body::Stall),
        ]);
        let server = TestServer::start(move |_: &Recorded| script.next()).await;
        for _ in 0..3 {
            let query = client(&server).catalog().query(&filter(3));
            // Clock frozen while the call runs: the bound fails only a call that waits for something
            // that never comes (then reported as stuck).
            let error = tokio::time::timeout(
                Duration::from_secs(1),
                paused::frozen(async {
                    if is_csv {
                        query.csv().send().await.unwrap_err()
                    } else {
                        query.geojson().send().await.unwrap_err()
                    }
                }),
            )
            .await
            .expect("head check must not await body");
            assert_eq!(concord_kind(&error), ErrorKind::Metadata);
        }
        assert_eq!(server.body_bytes_sent(), 0);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn transport_failure_before_headers_is_not_retried() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = client(&server)
        .catalog()
        .query(&filter(3))
        .geojson()
        .send()
        .await
        .unwrap_err();
    assert_eq!(concord_kind(&error), ErrorKind::Transport);
    let Error::Concord(inner) = error else {
        unreachable!()
    };
    assert!(inner.head().is_none());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn csv_failures_are_terminal_and_never_skip_bad_rows() {
    for (body, kind, good) in [
        (
            include_str!("../fixtures/bad-number.csv"),
            ReadErrorKind::Decode,
            1,
        ),
        (
            include_str!("../fixtures/bad-width.csv"),
            ReadErrorKind::Format,
            1,
        ),
        (
            include_str!("../fixtures/unclosed-quote.csv"),
            ReadErrorKind::Format,
            1,
        ),
        ("", ReadErrorKind::Format, 0),
        ("id,id\na,b\n", ReadErrorKind::Format, 0),
        ("id,place\na,b\n", ReadErrorKind::Decode, 0),
    ] {
        let server = TestServer::start(move |_: &Recorded| fragmented(body)).await;
        let mut reader = client(&server)
            .catalog()
            .query(&filter(3))
            .csv()
            .send()
            .await
            .unwrap()
            .data;
        for _ in 0..good {
            assert_eq!(
                reader.next().await.unwrap().unwrap().id.as_str(),
                "synthetic-a"
            );
        }
        let Error::Stream(error) = reader.next().await.unwrap().unwrap_err() else {
            panic!("expected stream error")
        };
        assert_eq!(error.kind(), kind);
        assert_eq!(error.position().records, good);
        assert_eq!(error.operation(), Some("Usgs.catalog.window.csv"));
        if good == 1 {
            assert_eq!(error.codec().unwrap().get("record"), Some(2));
        }
        assert!(error.is_terminal());
        assert!(!format!("{error:?} {error}").contains("secret-"));
        assert_eq!(reader.completion(), Completion::Failed);
        assert!(reader.next().await.is_none());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn csv_truncated_transfer_and_invalid_utf8_preserve_the_prefix() {
    for transport in [false, true] {
        let server = TestServer::start(move |_: &Recorded| {
            if transport {
                csv("").with_body(Body::FailAfter(vec![Bytes::copy_from_slice(
                    CSV1.as_bytes(),
                )]))
            } else {
                let mut bytes = CSV1.as_bytes().to_vec();
                let third = CSV2.strip_prefix(HEADER).unwrap();
                let mut third = third.as_bytes().to_vec();
                let at = third
                    .windows(b"Example Island".len())
                    .position(|w| w == b"Example Island")
                    .unwrap();
                third[at] = 0xff;
                bytes.extend_from_slice(&third);
                csv("").body(bytes)
            }
        })
        .await;
        let mut rows = client(&server)
            .catalog()
            .query(&filter(3))
            .csv()
            .send()
            .await
            .unwrap()
            .data;
        assert!(rows.next().await.unwrap().is_ok());
        assert!(rows.next().await.unwrap().is_ok());
        let Error::Stream(error) = rows.next().await.unwrap().unwrap_err() else {
            panic!("expected stream error")
        };
        assert_eq!(
            error.kind(),
            if transport {
                ReadErrorKind::Transport
            } else {
                ReadErrorKind::Format
            }
        );
        assert_eq!(error.position().records, 2);
        if !transport {
            assert!(error.codec().unwrap().label().contains("UTF-8"));
        }
        assert_eq!(rows.completion(), Completion::Failed);
        assert!(rows.next().await.is_none());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn csv_size_limits_are_errors_not_clean_eof() {
    let oversized_record = format!("{HEADER}{}\n", "x".repeat(CSV_RECORD_LIMIT + 1));
    let oversized_header = format!("{}\n", "x".repeat(4097));
    for body in [oversized_record, oversized_header] {
        let server = TestServer::start(move |_: &Recorded| csv(&body)).await;
        let mut rows = client(&server)
            .catalog()
            .query(&filter(3))
            .csv()
            .send()
            .await
            .unwrap()
            .data;
        assert_eq!(
            read_kind(rows.next().await.unwrap().unwrap_err()),
            ReadErrorKind::Limit
        );
        assert_eq!(rows.completion(), Completion::Failed);
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| {
        let mut remaining = CSV_BODY_LIMIT / 4096 + 2;
        csv("").with_body(Body::Generate(
            Box::new(move || {
                if remaining == 0 {
                    return None;
                }
                remaining -= 1;
                Some(Bytes::from(vec![b'\n'; 4096]))
            }),
            None,
        ))
    })
    .await;
    let mut rows = client(&server)
        .catalog()
        .query(&filter(3))
        .csv()
        .send()
        .await
        .unwrap()
        .data;
    assert_eq!(
        read_kind(rows.next().await.unwrap().unwrap_err()),
        ReadErrorKind::Limit
    );
    assert_eq!(rows.completion(), Completion::Failed);
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_invalid_repeated_and_http_date_are_exposed_without_sleeping() {
    for (values, expected) in [
        (
            vec!["secret-invalid"],
            Err(concord_sdk_usgs_earthquake_catalog::concord_retry::http::RetryAfterError::Invalid),
        ),
        (
            vec!["1", "2"],
            Err(
                concord_sdk_usgs_earthquake_catalog::concord_retry::http::RetryAfterError::Repeated,
            ),
        ),
        (
            vec!["18446744073709551616"],
            Err(
                concord_sdk_usgs_earthquake_catalog::concord_retry::http::RetryAfterError::Overflow,
            ),
        ),
        (
            vec!["Thu, 01 Jan 1970 00:00:10 GMT"],
            Ok(Some(Duration::from_secs(10))),
        ),
        (vec![], Ok(None)),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            let mut reply = Reply::text(429, "text/plain", ERROR_TEXT);
            for value in &values {
                reply = reply.header("retry-after", *value);
            }
            reply
        })
        .await;
        let Error::Http(error) = client(&server)
            .catalog()
            .query(&filter(3))
            .csv()
            .send()
            .await
            .unwrap_err()
        else {
            panic!()
        };
        assert_eq!(error.retry_after(UNIX_EPOCH), expected);
        assert!(!format!("{error:?}").contains("secret-"));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn inconsistent_pages_and_offset_overflow_are_explicit() {
    for (body, from) in [
        (
            PAGE1.replace("\"count\": 2", "\"count\": 7"),
            NonZeroU64::MIN,
        ),
        (WINDOW.to_owned(), NonZeroU64::MIN),
        (PAGE1.to_owned(), NonZeroU64::MAX),
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, &body)).await;
        let mut pages =
            client(&server)
                .catalog()
                .pages(&filter(2), from, NonZeroUsize::new(2).unwrap());
        let error = pages.next_page().await.unwrap_err();
        assert!(matches!(error.provider_error(), Some(Error::Protocol(_))));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[test]
fn fixture_constructor_refuses_nonloopback_destinations() {
    for bad in [
        "https://127.0.0.1:1234",
        "http://192.0.2.1:1234",
        "http://localhost:1234",
        "http://127.0.0.1:1234/path",
        "http://127.0.0.1:1234?x=1",
    ] {
        assert!(Usgs::for_fixture(reqwest::Client::new(), bad).is_err());
    }
    for good in ["http://127.0.0.1:1234", "http://[::1]:1234"] {
        let api = Usgs::for_fixture(reqwest::Client::new(), good).unwrap();
        let prepared = api
            .catalog()
            .event(EventId::new("synthetic-event").unwrap())
            .prepare()
            .unwrap();
        assert_eq!(
            prepared.request().url().host_str(),
            reqwest::Url::parse(good).unwrap().host_str()
        );
    }
}
