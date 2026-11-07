//! Maintenance regressions and a consumer of the shared FDSN filter contract.
mod common;

use common::*;
use concord_sdk_usgs_earthquake_catalog::{
    Error, Magnitude, OrderBy, Timestamp, Window,
    concord::bytes::Bytes,
    concord_codecs::stream::{Completion, ReadErrorKind},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

#[tokio::test]
async fn all_orderings_and_optional_magnitude_bounds_match_in_both_formats() {
    let server = TestServer::start(|r: &Recorded| {
        if r.query_all("format") == ["csv"] {
            csv(HEADER)
        } else {
            Reply::json(200, include_str!("../fixtures/empty.geojson"))
        }
    })
    .await;
    let catalog = client(&server).catalog();
    for (order, wire) in [
        (OrderBy::Time, "time"),
        (OrderBy::TimeAsc, "time-asc"),
        (OrderBy::Magnitude, "magnitude"),
        (OrderBy::MagnitudeAsc, "magnitude-asc"),
    ] {
        // Negative, equal and large finite values are valid SDK inputs; the provider does
        // not publish a numeric magnitude range. No scientific clamping is appropriate.
        for (min, max, min_wire, max_wire) in [
            (None, None, None, None),
            (Some(-1.25), None, Some("-1.25"), None),
            (None, Some(12.5), None, Some("12.5")),
            (Some(2.5), Some(6.75), Some("2.5"), Some("6.75")),
            (Some(-1.25), Some(-1.25), Some("-1.25"), Some("-1.25")),
        ] {
            let mut window = Window::new("2014-01-01T00:00:00Z", "2014-01-02T00:00:00Z")
                .unwrap()
                .with_order_by(order);
            if let Some(min) = min {
                window = window.min_magnitude(Magnitude::new(min).unwrap());
            }
            if let Some(max) = max {
                window = window.max_magnitude(Magnitude::new(max).unwrap());
            }
            window.validate().unwrap();
            assert_eq!(window.order_by(), order);
            let query = catalog.query(&window);
            query.geojson().send().await.unwrap();
            let mut rows = query.csv().send().await.unwrap().data;
            assert!(rows.next().await.is_none());
            assert_eq!(rows.completion(), Completion::EndOfInput);

            let requests = server.requests();
            let pair = &requests[requests.len() - 2..];
            for request in pair {
                assert_eq!(request.query_all("orderby"), [wire]);
                assert_eq!(
                    request.query_all("minmagnitude"),
                    min_wire.into_iter().collect::<Vec<_>>()
                );
                assert_eq!(
                    request.query_all("maxmagnitude"),
                    max_wire.into_iter().collect::<Vec<_>>()
                );
            }
            let filters = |r: &Recorded| {
                r.query_pairs()
                    .into_iter()
                    .filter(|(key, _)| key != "format")
                    .collect::<Vec<_>>()
            };
            assert_eq!(filters(&pair[0]), filters(&pair[1]));
        }
    }
    assert_eq!(server.requests().len(), 40);
    server.shutdown().await;
}

#[tokio::test]
async fn inverted_bounds_from_builders_and_generated_setters_fail_before_io() {
    let server = TestServer::start(|_: &Recorded| panic!("invalid bounds reached I/O")).await;
    let catalog = client(&server).catalog();
    let low = Magnitude::new(3.0).unwrap();
    let high = Magnitude::new(5.0).unwrap();
    for window in [
        filter(2).min_magnitude(high).max_magnitude(low),
        filter(2).max_magnitude(low).min_magnitude(high),
    ] {
        assert!(matches!(window.validate(), Err(Error::InvalidInput(_))));
        let query = catalog.query(&window);
        assert!(matches!(
            query.geojson().send().await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            query.csv().send().await,
            Err(Error::InvalidInput(_))
        ));
        let mut pages = catalog.pages(&window, NonZeroU64::MIN, NonZeroUsize::MIN);
        let error = pages.next_page().await.unwrap_err();
        assert!(matches!(
            error.provider_error(),
            Some(Error::InvalidInput(_))
        ));
        let mut export = catalog.export(&window, NonZeroU64::MIN, NonZeroUsize::MIN);
        assert!(matches!(export.next().await, Err(Error::InvalidInput(_))));
        assert_eq!(export.rows_delivered(), 0);
        assert_eq!(export.completion(), Completion::Failed);
    }

    let valid = filter(2).min_magnitude(low).max_magnitude(high);
    valid.validate().unwrap();
    for scope in [
        catalog
            .query(&valid)
            .set_minmagnitude(Magnitude::new(6.0).unwrap()),
        catalog
            .query(&valid)
            .set_maxmagnitude(Magnitude::new(2.0).unwrap()),
        catalog
            .window(
                Timestamp::parse("2014-01-01T00:00:00Z").unwrap(),
                Timestamp::parse("2014-01-02T00:00:00Z").unwrap(),
            )
            .set_minmagnitude(high)
            .set_maxmagnitude(low),
    ] {
        assert!(matches!(
            scope.geojson().send().await,
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            scope.csv().send().await,
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

/// Consumer: reuse one filtered window for a paged list, selected-event lookup and CSV.
#[tokio::test]
async fn filtered_window_lookup_and_incremental_csv_keep_filters_across_pages() {
    // Repartition the authored fixtures in descending origin-time order (C, B, A).
    // JSON values and CSV records are rearranged independently of SDK models/serializers.
    let collection: serde_json::Value = serde_json::from_str(WINDOW).unwrap();
    let mut first_json = collection.clone();
    first_json["features"] =
        serde_json::json!([collection["features"][2], collection["features"][1]]);
    first_json["metadata"]["count"] = 2.into();
    let mut last_json = collection.clone();
    last_json["features"] = serde_json::json!([collection["features"][0]]);
    last_json["metadata"]["count"] = 1.into();
    let split = CSV1.find("2014-01-01T02:00:00Z").unwrap();
    let first_csv = format!(
        "{HEADER}{}\n{}",
        CSV2.strip_prefix(HEADER).unwrap(),
        &CSV1[split..]
    );
    let last_csv = format!("{HEADER}{}", &CSV1[HEADER.len()..split]);
    let first_csv_len = first_csv.len() as u64;
    let server = TestServer::start(move |r: &Recorded| {
        if r.query_all("eventid") == ["synthetic-b"] {
            return Reply::json(200, PAGE1_FEATURE_B);
        }
        let first = r.query_all("offset") == ["1"];
        if r.query_all("format") == ["geojson"] {
            return Reply::json(
                200,
                if first {
                    first_json.to_string()
                } else {
                    last_json.to_string()
                },
            );
        }
        if !first {
            return fragmented(&last_csv);
        }
        let split = first_csv.find("2014-01-01T02:00:00Z").unwrap();
        let mut chunks = vec![(
            Duration::ZERO,
            Bytes::copy_from_slice(&first_csv.as_bytes()[..split]),
        )];
        chunks.extend(
            first_csv.as_bytes()[split..]
                .iter()
                .enumerate()
                .map(|(i, byte)| {
                    (
                        if i == 0 {
                            Duration::from_secs(2)
                        } else {
                            Duration::ZERO
                        },
                        Bytes::copy_from_slice(&[*byte]),
                    )
                }),
        );
        csv("").with_body(Body::Chunks(chunks))
    })
    .await;
    let catalog = client(&server).catalog();
    let window = filter(2)
        .min_magnitude(Magnitude::new(4.4).unwrap())
        .max_magnitude(Magnitude::new(5.5).unwrap())
        .with_order_by(OrderBy::Time);
    window.validate().unwrap();
    let budget = NonZeroUsize::new(3).unwrap();
    let events = catalog
        .pages(&window, NonZeroU64::MIN, budget)
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(
        events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<Vec<_>>(),
        ["synthetic-c", "synthetic-b", "synthetic-a"]
    );
    let chosen = catalog.event(events[1].id.clone()).send().await.unwrap();
    assert_eq!(chosen.data.id.as_str(), "synthetic-b");
    assert_eq!(chosen.data.properties.mag, Some(5.1));

    let before = server.body_bytes_sent();
    let mut export = catalog.export(&window, NonZeroU64::MIN, budget);
    assert_eq!(export.pages_requested(), 0);
    let first = tokio::time::timeout(Duration::from_secs(1), export.next())
        .await
        .expect("first row must arrive before the delayed suffix")
        .unwrap()
        .unwrap();
    assert_eq!(first.id, events[0].id);
    assert_eq!(first.place, "Example Island");
    assert!(server.body_bytes_sent() - before < first_csv_len);
    assert_eq!(export.completion(), Completion::Reading);
    let second = export.next().await.unwrap().unwrap();
    assert_eq!(second.id, events[1].id);
    assert_eq!(second.place, "Ridge, \"North\"\nnear café");
    assert_eq!(second.mag, Some(5.1));
    let third = export.next().await.unwrap().unwrap();
    assert_eq!(third.id, events[2].id);
    assert_eq!(third.place, "10 km west of Example, California");
    assert!(export.next().await.unwrap().is_none());
    assert_eq!(export.completion(), Completion::EndOfInput);
    assert_eq!(export.rows_delivered(), 3);

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    for (json, csv, offset) in [
        (&requests[0], &requests[3], "1"),
        (&requests[1], &requests[4], "3"),
    ] {
        for request in [json, csv] {
            assert_eq!(request.query_all("minmagnitude"), ["4.4"]);
            assert_eq!(request.query_all("maxmagnitude"), ["5.5"]);
            assert_eq!(request.query_all("orderby"), ["time"]);
            assert_eq!(request.query_all("offset"), [offset]);
        }
        let filters = |r: &Recorded| {
            r.query_pairs()
                .into_iter()
                .filter(|(key, _)| key != "format")
                .collect::<Vec<_>>()
        };
        assert_eq!(filters(json), filters(csv));
    }
    assert_eq!(
        requests[2].query_pairs(),
        [
            ("nodata".into(), "404".into()),
            ("eventid".into(), "synthetic-b".into()),
            ("format".into(), "geojson".into()),
        ]
    );
    server.shutdown().await;
}

// Independently authored detail, using the same synthetic event as the existing CSV fixture.
const PAGE1_FEATURE_B: &str = r#"{"type":"Feature","id":"synthetic-b","properties":{"mag":5.1,"place":"Ridge, \"North\"\nnear café","time":1388541600000,"updated":1388542200000,"type":"earthquake","status":"reviewed"},"geometry":{"type":"Point","coordinates":[-122.5,37.5,8.25]}}"#;

#[tokio::test]
async fn filtered_csv_still_stops_at_the_first_bad_row() {
    let server =
        TestServer::start(|_: &Recorded| fragmented(include_str!("../fixtures/bad-number.csv")))
            .await;
    let window = filter(3)
        .max_magnitude(Magnitude::new(6.0).unwrap())
        .with_order_by(OrderBy::Magnitude);
    let mut export = client(&server)
        .catalog()
        .export(&window, NonZeroU64::MIN, NonZeroUsize::MIN);
    assert_eq!(
        export.next().await.unwrap().unwrap().id.as_str(),
        "synthetic-a"
    );
    let Error::Stream(error) = export.next().await.unwrap_err() else {
        panic!("bad row must be reported")
    };
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(error.position().records, 1);
    assert_eq!(export.rows_delivered(), 1);
    assert_eq!(export.completion(), Completion::Failed);
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
