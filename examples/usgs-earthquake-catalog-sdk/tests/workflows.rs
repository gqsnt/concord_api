//! Connected local workflows, explicit partial results, and incremental delivery evidence.
mod common;

use common::*;
use concord_sdk_usgs_earthquake_catalog::{
    Error, EventId,
    concord::bytes::Bytes,
    concord_codecs::stream::{Completion, ReadErrorKind},
    concord_pagination::{PagesError, Resume},
};
use concord_testserver::{Body, Recorded, Reply, TestServer, paused};
use std::{
    num::{NonZeroU64, NonZeroUsize},
    time::Duration,
};

fn budget(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[tokio::test]
async fn bounded_geojson_window_then_selected_event() {
    let server = TestServer::start(|request: &Recorded| {
        if request.query_all("eventid") == ["synthetic-a"] {
            Reply::json(200, EVENT)
        } else {
            match request.query_all("offset").as_slice() {
                [offset] if offset == "1" => Reply::json(200, PAGE1),
                [offset] if offset == "3" => Reply::json(200, PAGE2),
                _ => panic!("unexpected request: {request:?}"),
            }
        }
    })
    .await;
    let catalog = client(&server).catalog();
    let mut pages = catalog.pages(&filter(2), NonZeroU64::MIN, budget(3));
    assert!(server.requests().is_empty());
    let mut events = Vec::new();
    while let Some(page) = pages.next_page().await.unwrap() {
        events.extend(page.items);
    }
    assert_eq!(
        events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["synthetic-a", "synthetic-b", "synthetic-c"]
    );
    assert!(pages.checkpoint().is_none());
    let chosen = catalog.event(events[0].id.clone()).send().await.unwrap();
    assert_eq!(chosen.data.id, events[0].id);
    assert_eq!(chosen.data.properties.mag, Some(4.5));
    assert!(
        chosen
            .data
            .properties
            .products
            .unwrap()
            .get("origin")
            .is_some()
    );
    assert_eq!(
        chosen.data.geometry.unwrap().coordinates,
        [-122.5, 37.5, 8.25]
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].query_all("offset"), ["1"]);
    assert_eq!(requests[1].query_all("offset"), ["3"]);
    for key in [
        "starttime",
        "endtime",
        "minmagnitude",
        "limit",
        "offset",
        "orderby",
        "eventtype",
    ] {
        assert!(requests[2].query_all(key).is_empty());
    }
    server.shutdown().await;
}

#[tokio::test]
async fn same_filtered_window_exports_rows_before_the_response_finishes() {
    let server = TestServer::start(|request: &Recorded| {
        let first = request.query_all("offset") == ["1"];
        if request.query_all("format") == ["geojson"] {
            return Reply::json(200, if first { PAGE1 } else { PAGE2 });
        }
        if !first {
            return fragmented(CSV2);
        }
        // Independently authored boundary after the first data record. The remaining
        // response includes UTF-8, escaped quotes and a literal newline, split at every byte.
        let split = CSV1.find("2014-01-01T02:00:00Z").unwrap();
        let mut chunks = vec![(
            Duration::ZERO,
            Bytes::copy_from_slice(&CSV1.as_bytes()[..split]),
        )];
        for (index, byte) in CSV1.as_bytes()[split..].iter().enumerate() {
            chunks.push((
                if index == 0 {
                    Duration::from_secs(2)
                } else {
                    Duration::ZERO
                },
                Bytes::copy_from_slice(&[*byte]),
            ));
        }
        csv("").with_body(Body::Chunks(chunks))
    })
    .await;
    let catalog = client(&server).catalog();
    let filter = filter(2);
    let listed = catalog
        .pages(&filter, NonZeroU64::MIN, budget(3))
        .items()
        .collect_complete()
        .await
        .unwrap();
    let before_csv = server.body_bytes_sent();
    let mut export = catalog.export(&filter, NonZeroU64::MIN, budget(3));
    assert_eq!(export.pages_requested(), 0);
    // Clock frozen while the call runs: the bound fails only a call that waits for something
    // that never comes (then reported as stuck).
    let first = tokio::time::timeout(Duration::from_secs(1), paused::frozen(export.next()))
        .await
        .expect("first row must arrive while the suffix is withheld")
        .unwrap()
        .unwrap();
    assert_eq!(first.id.as_str(), "synthetic-a");
    assert_eq!(first.place, "10 km west of Example, California");
    assert!(server.body_bytes_sent() - before_csv < CSV1.len() as u64);
    assert_eq!(export.completion(), Completion::Reading);
    assert_eq!(export.rows_delivered(), 1);
    assert_eq!(export.pages_requested(), 1);
    let second = export.next().await.unwrap().unwrap();
    assert_eq!(second.place, "Ridge, \"North\"\nnear café");
    assert_eq!(second.mag, Some(5.1));
    assert_eq!(second.nst, None);
    let third = export.next().await.unwrap().unwrap();
    assert_eq!(third.id.as_str(), "synthetic-c");
    assert_eq!(third.mag, Some(4.8));
    assert!(export.next().await.unwrap().is_none());
    assert!(export.next().await.unwrap().is_none());
    assert_eq!(export.completion(), Completion::EndOfInput);
    assert_eq!(export.pages_requested(), 2);
    assert_eq!(export.rows_delivered(), 3);
    let ids = [first.id, second.id, third.id];
    assert_eq!(
        ids,
        listed
            .into_iter()
            .map(|event| event.id)
            .collect::<Vec<_>>()
            .as_slice()
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    for (json, csv) in [(&requests[0], &requests[2]), (&requests[1], &requests[3])] {
        let filter_pairs = |r: &Recorded| {
            r.query_pairs()
                .into_iter()
                .filter(|(key, _)| key != "format")
                .collect::<Vec<_>>()
        };
        assert_eq!(filter_pairs(json), filter_pairs(csv));
    }
    server.shutdown().await;
}

#[tokio::test]
async fn export_surfaces_bad_row_after_delivering_its_valid_prefix() {
    let server =
        TestServer::start(|_: &Recorded| fragmented(include_str!("../fixtures/bad-number.csv")))
            .await;
    let mut export = client(&server)
        .catalog()
        .export(&filter(3), NonZeroU64::MIN, budget(2));
    assert_eq!(
        export.next().await.unwrap().unwrap().id.as_str(),
        "synthetic-a"
    );
    let error = export.next().await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains("secret-invalid-number"));
    let Error::Stream(error) = error else {
        panic!("bad numeric row must be surfaced")
    };
    assert_eq!(error.kind(), ReadErrorKind::Decode);
    assert_eq!(error.position().records, 1);
    assert_eq!(error.codec().unwrap().get("record"), Some(2));
    assert_eq!(export.completion(), Completion::Failed);
    assert_eq!(export.rows_delivered(), 1);
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn page_budgets_report_incomplete_results_and_one_based_continuation() {
    let server = TestServer::start(|request: &Recorded| {
        let first = request.query_all("offset") == ["1"];
        if request.query_all("format") == ["csv"] {
            csv(if first { CSV1 } else { CSV2 })
        } else {
            Reply::json(200, if first { PAGE1 } else { PAGE2 })
        }
    })
    .await;
    let catalog = client(&server).catalog();
    let mut pages = catalog.pages(&filter(2), NonZeroU64::MIN, budget(1));
    assert_eq!(pages.next_page().await.unwrap().unwrap().items.len(), 2);
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(error, PagesError::Bounded { .. }));
    assert_eq!(
        error.resume(),
        Some(&Resume::At(NonZeroU64::new(3).unwrap()))
    );
    assert!(pages.next_page().await.is_err());
    let mut export = catalog.export(&filter(2), NonZeroU64::MIN, budget(1));
    assert!(export.next().await.unwrap().is_some());
    assert!(export.next().await.unwrap().is_some());
    assert!(
        matches!(export.next().await,Err(Error::PageBudget { next_offset }) if next_offset.get() == 3)
    );
    assert_eq!(export.rows_delivered(), 2);
    assert_eq!(export.completion(), Completion::Failed);
    assert!(matches!(export.next().await, Err(Error::ExportFailed)));
    assert_eq!(server.requests().len(), 2);
    let mut resumed = catalog.export(&filter(2), NonZeroU64::new(3).unwrap(), budget(1));
    assert_eq!(
        resumed.next().await.unwrap().unwrap().id.as_str(),
        "synthetic-c"
    );
    assert!(resumed.next().await.unwrap().is_none());
    assert_eq!(server.requests().len(), 3);
    server.shutdown().await;
}

#[tokio::test]
async fn empty_windows_and_nodata_end_traversals_but_lookup_stays_an_error() {
    for status in [200, 204, 404] {
        let server = TestServer::start(move |request: &Recorded| {
            if !request.query_all("eventid").is_empty() {
                return Reply::new(404);
            }
            if status != 200 {
                return Reply::new(status);
            }
            if request.query_all("format") == ["csv"] {
                csv(HEADER)
            } else {
                Reply::json(200, include_str!("../fixtures/empty.geojson"))
            }
        })
        .await;
        let catalog = client(&server).catalog();
        let events = catalog
            .pages(&filter(2), NonZeroU64::MIN, budget(1))
            .items()
            .collect_complete()
            .await
            .unwrap();
        assert!(events.is_empty());
        let mut export = catalog.export(&filter(2), NonZeroU64::MIN, budget(1));
        assert!(export.next().await.unwrap().is_none());
        assert_eq!(export.completion(), Completion::EndOfInput);
        assert!(
            catalog
                .event(EventId::new("missing-event").unwrap())
                .send()
                .await
                .unwrap_err()
                .is_no_data()
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn exact_multiple_needs_a_final_empty_request_and_updates_can_repeat_events() {
    for mutated in [false, true] {
        let server = TestServer::start(move |request: &Recorded| {
            if request.query_all("offset") == ["1"] {
                return Reply::json(200, PAGE1);
            }
            assert_eq!(request.query_all("offset"), ["3"]);
            if mutated {
                Reply::json(200, include_str!("../fixtures/page-mutated.geojson"))
            } else {
                Reply::new(404)
            }
        })
        .await;
        let listed = client(&server)
            .catalog()
            .pages(&filter(2), NonZeroU64::MIN, budget(2))
            .items()
            .collect_complete()
            .await
            .unwrap();
        let ids: Vec<_> = listed.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            if mutated {
                vec!["synthetic-a", "synthetic-b", "synthetic-b"]
            } else {
                vec!["synthetic-a", "synthetic-b"]
            }
        );
        assert_eq!(server.requests().len(), 2);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn failed_second_pages_preserve_delivered_rows_and_do_not_replay() {
    let server = TestServer::start(|request: &Recorded| {
        if request.query_all("offset") != ["1"] {
            return Reply::text(503, "text/plain", ERROR_TEXT);
        }
        if request.query_all("format") == ["csv"] {
            csv(CSV1)
        } else {
            Reply::json(200, PAGE1)
        }
    })
    .await;
    let catalog = client(&server).catalog();
    let incomplete = catalog
        .pages(&filter(2), NonZeroU64::MIN, budget(3))
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 2);
    assert!(matches!(
        incomplete.error.provider_error(),
        Some(Error::Http(_))
    ));
    let mut export = catalog.export(&filter(2), NonZeroU64::MIN, budget(3));
    export.next().await.unwrap().unwrap();
    export.next().await.unwrap().unwrap();
    assert!(matches!(export.next().await, Err(Error::Http(_))));
    assert_eq!(export.rows_delivered(), 2);
    assert_eq!(export.completion(), Completion::Failed);
    assert_eq!(server.requests().len(), 4);
    server.shutdown().await;
}

#[tokio::test]
async fn crlf_inside_quotes_survives_and_dropping_export_stops_admission() {
    let crlf = CSV1.replace('\n', "\r\n");
    let server = TestServer::start(move |_: &Recorded| fragmented(&crlf)).await;
    let mut export = client(&server)
        .catalog()
        .export(&filter(2), NonZeroU64::MIN, budget(3));
    export.next().await.unwrap().unwrap();
    let second = export.next().await.unwrap().unwrap();
    assert_eq!(second.place, "Ridge, \"North\"\r\nnear café");
    drop(export);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn csv_overfull_page_and_offset_overflow_do_not_report_completion() {
    for (size, from) in [(1, NonZeroU64::MIN), (2, NonZeroU64::MAX)] {
        let server = TestServer::start(|_: &Recorded| csv(CSV1)).await;
        let mut export = client(&server)
            .catalog()
            .export(&filter(size), from, budget(3));
        for _ in 0..size {
            export.next().await.unwrap().unwrap();
        }
        assert!(matches!(export.next().await, Err(Error::Protocol(_))));
        assert_eq!(export.completion(), Completion::Failed);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}
