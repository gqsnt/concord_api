//! Dependent chains (P01, P05, P06, P07): key-gated construction, empty pages, budget charging,
//! failure identity, cancellation and typed projections.

mod support;

use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{Budget, Outcome, Page, Resume, sequential};
use support::{Server, now};

fn budget(n: usize) -> Budget {
    Budget::new(NonZeroUsize::new(n).unwrap())
}

type Reply = Result<Page<u32, String>, &'static str>;

fn page(items: &[u32], next: Option<&str>) -> Reply {
    Ok(Page::new(items.to_vec(), next.map(str::to_owned)))
}

#[test]
fn each_request_is_constructed_only_once_its_key_is_known() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));

    // Construction performs no call.
    assert_eq!(server.call_count(), 0);

    // The first poll admits the first page only.
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [None]);
    // Polling again while it is pending admits nothing else (no key is known yet).
    assert!(now(pages.next()).is_none());
    assert_eq!(server.call_count(), 1);

    server.open(&None, page(&[1, 2], Some("opaque-c1")));
    let first = now(pages.next()).unwrap().unwrap().unwrap();
    assert_eq!(first.items, [1, 2]);
    // The page was emitted; the next request is not built before the caller asks.
    assert_eq!(server.call_count(), 1);
    assert_eq!(
        pages.checkpoint(),
        Some(&Resume::At("opaque-c1".to_owned()))
    );

    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [None, Some("opaque-c1".to_owned())]);
    server.open(&Some("opaque-c1".to_owned()), page(&[3], None));
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().items, [3]);
    assert!(now(pages.next()).unwrap().is_none());

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert!(report.resume.is_none());
    assert!(report.failed.is_none());
    assert_eq!(report.progress.admitted, 2);
    assert_eq!(server.call_count(), 2);
}

#[test]
fn an_empty_page_with_a_next_key_continues() {
    let mut pages = sequential(Resume::Start, budget(10), |cursor: Option<String>| {
        ready(match cursor.as_deref() {
            None => page(&[1], Some("a")),
            Some("a") => page(&[], Some("b")),
            Some("b") => page(&[], Some("c")),
            _ => page(&[2], None),
        })
    });
    let mut seen = Vec::new();
    while let Some(next) = now(pages.next()).unwrap() {
        seen.push(next.unwrap().items);
    }
    assert_eq!(seen, [vec![1], vec![], vec![], vec![2]]);
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
}

#[test]
fn the_budget_is_charged_at_admission_and_ends_a_repeating_cursor() {
    let mut calls = 0;
    // A provider that keeps returning the same cursor.
    let mut pages = sequential(Resume::Start, budget(3), |_cursor: Option<String>| {
        calls += 1;
        ready(page(&[7], Some("same")))
    });
    let mut emitted = 0;
    while let Some(next) = now(pages.next()).unwrap() {
        next.unwrap();
        emitted += 1;
    }
    assert_eq!(emitted, 3);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.resume, Some(Resume::At("same".to_owned())));
    assert_eq!(report.progress.admitted, 3);
    assert_eq!(calls, 3);
}

#[test]
fn a_failing_page_is_charged_and_cannot_be_used_to_exceed_the_budget() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(2), |cursor| server.call(cursor));
    assert!(now(pages.next()).is_none());
    server.open(&None, page(&[1], Some("k2")));
    now(pages.next()).unwrap().unwrap().unwrap();
    assert!(now(pages.next()).is_none());
    assert_eq!(pages.progress().admitted, 2);
    server.open(&Some("k2".to_owned()), Err("server error"));
    assert_eq!(
        now(pages.next()).unwrap().unwrap().unwrap_err(),
        "server error"
    );
    assert_eq!(pages.progress().admitted, 2);
    assert!(now(pages.next()).unwrap().is_none());
    assert_eq!(server.call_count(), 2);
}

#[test]
fn a_failure_is_terminal_and_keeps_the_failed_page_apart_from_the_resume_point() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));
    assert!(now(pages.next()).is_none());
    server.open(&None, page(&[1], Some("k2")));
    now(pages.next()).unwrap().unwrap().unwrap();
    assert!(now(pages.next()).is_none());
    server.open(&Some("k2".to_owned()), Err("expired cursor"));

    // The typed error is returned as is.
    assert_eq!(
        now(pages.next()).unwrap().unwrap().unwrap_err(),
        "expired cursor"
    );
    // No continuation is guessed: the chain is over.
    assert!(now(pages.next()).unwrap().is_none());
    assert!(now(pages.next()).unwrap().is_none());
    assert_eq!(server.call_count(), 2);
    assert_eq!(pages.checkpoint(), Some(&Resume::At("k2".to_owned())));

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(Resume::At("k2".to_owned())));
    assert_eq!(report.resume, Some(Resume::At("k2".to_owned())));
    assert_eq!(report.progress.emitted, 1);
    assert_eq!(report.progress.failed, 1);
}

#[test]
fn a_failure_of_the_first_page_names_the_start() {
    let mut pages = sequential(Resume::Start, budget(10), |_: Option<String>| {
        ready(Err::<Page<u32, String>, _>("down"))
    });
    assert!(now(pages.next()).unwrap().unwrap().is_err());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(Resume::Start));
    assert_eq!(report.resume, Some(Resume::Start));
}

#[test]
fn resuming_at_a_key_starts_there() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::At("k5".to_owned()), budget(10), |cursor| {
        server.call(cursor)
    });
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [Some("k5".to_owned())]);
}

#[test]
fn finish_drops_the_page_in_flight_and_nothing_is_admitted_afterwards() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));
    assert!(now(pages.next()).is_none());
    server.open(&None, page(&[1], Some("k2")));
    now(pages.next()).unwrap().unwrap().unwrap();
    assert!(now(pages.next()).is_none());
    assert_eq!(server.active(), 1);

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    // The dropped page is where to resume; it did not fail.
    assert_eq!(report.resume, Some(Resume::At("k2".to_owned())));
    assert!(report.failed.is_none());
    assert_eq!(report.progress.cancelled, 1);
    assert!(server.dropped(&Some("k2".to_owned())));
    assert_eq!(server.active(), 0);
    assert_eq!(server.call_count(), 2);
}

#[test]
fn dropping_the_traversal_drops_its_future() {
    let server = Server::<Option<String>, Reply>::new();
    {
        let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));
        assert!(now(pages.next()).is_none());
        assert_eq!(server.active(), 1);
    }
    assert!(server.dropped(&None));
    assert_eq!(server.active(), 0);
    assert_eq!(server.call_count(), 1);
}

#[test]
fn a_cancelled_next_keeps_the_page_in_flight() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));
    for _ in 0..5 {
        // Each `now` drops a pending `next` future.
        assert!(now(pages.next()).is_none());
    }
    assert_eq!(server.call_count(), 1);
    server.open(&None, page(&[1], None));
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().items, [1]);
    assert_eq!(server.call_count(), 1);
}

#[test]
fn finish_without_observing_the_end_reports_the_natural_outcome() {
    let mut pages = sequential(Resume::Start, budget(1), |_: Option<String>| {
        ready(page(&[1], Some("more")))
    });
    now(pages.next()).unwrap().unwrap().unwrap();
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.resume, Some(Resume::At("more".to_owned())));

    let mut pages = sequential(Resume::Start, budget(5), |_: Option<String>| {
        ready(page(&[1], None))
    });
    now(pages.next()).unwrap().unwrap().unwrap();
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
}

// --- typed projections (P05): the adapter maps a full envelope, selects the continuation from
// --- a header-like field before moving nested records out; records are not `Clone`.

/// A provider record that is deliberately not `Clone`.
#[derive(Debug, PartialEq)]
struct Record {
    id: u32,
}

struct Envelope {
    data: Data,
    /// Stands in for a response header (for example `Link` or `x-next-cursor`).
    next_header: Option<String>,
    request_id: String,
}

struct Data {
    records: Vec<Record>,
    has_more: bool,
}

#[derive(Debug, PartialEq)]
enum ApiError {
    /// The envelope contradicts itself.
    Protocol(&'static str),
}

/// What an SDK adapter does: continuation from the header, items moved from the nested field,
/// selected metadata kept, an inconsistent envelope refused.
fn project(envelope: Envelope) -> Result<Page<Record, String, String>, ApiError> {
    if envelope.data.has_more && envelope.next_header.is_none() {
        return Err(ApiError::Protocol("has_more without a next cursor"));
    }
    Ok(Page {
        next: envelope.next_header,
        items: envelope.data.records,
        meta: envelope.request_id,
    })
}

#[test]
fn a_typed_envelope_projection_moves_records_and_keeps_metadata() {
    let mut pages = sequential(Resume::Start, budget(10), |cursor: Option<String>| {
        let envelope = match cursor.as_deref() {
            None => Envelope {
                data: Data {
                    records: vec![Record { id: 1 }, Record { id: 2 }],
                    has_more: true,
                },
                next_header: Some("h2".into()),
                request_id: "req-1".into(),
            },
            _ => Envelope {
                data: Data {
                    records: vec![Record { id: 3 }],
                    has_more: false,
                },
                next_header: None,
                request_id: "req-2".into(),
            },
        };
        ready(project(envelope))
    });
    let mut records = Vec::new();
    let mut request_ids = Vec::new();
    while let Some(next) = now(pages.next()).unwrap() {
        let page = next.unwrap();
        request_ids.push(page.meta);
        records.extend(page.items);
    }
    assert_eq!(
        records,
        [Record { id: 1 }, Record { id: 2 }, Record { id: 3 }]
    );
    assert_eq!(request_ids, ["req-1", "req-2"]);
}

#[test]
fn an_inconsistent_envelope_is_a_typed_page_failure_with_defined_progress() {
    let mut pages = sequential(Resume::Start, budget(10), |cursor: Option<String>| {
        let envelope = match cursor.as_deref() {
            None => Envelope {
                data: Data {
                    records: vec![Record { id: 1 }],
                    has_more: true,
                },
                next_header: Some("h2".into()),
                request_id: "req-1".into(),
            },
            _ => Envelope {
                data: Data {
                    records: vec![Record { id: 2 }],
                    has_more: true,
                },
                next_header: None,
                request_id: "req-2".into(),
            },
        };
        ready(project(envelope))
    });
    assert_eq!(
        now(pages.next()).unwrap().unwrap().unwrap().items,
        [Record { id: 1 }]
    );
    assert_eq!(
        now(pages.next()).unwrap().unwrap().unwrap_err(),
        ApiError::Protocol("has_more without a next cursor")
    );
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(Resume::At("h2".to_owned())));
    assert_eq!(report.progress.emitted, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn real_wakeups_drive_the_chain() {
    let server = Server::<Option<String>, Reply>::new();
    let mut pages = sequential(Resume::Start, budget(10), |cursor| server.call(cursor));
    let consumer = async {
        let mut items = Vec::new();
        while let Some(next) = pages.next().await {
            items.extend(next.unwrap().items);
        }
        items
    };
    let driver = async {
        for (key, reply) in [
            (None, page(&[1], Some("a"))),
            (Some("a".to_owned()), page(&[], Some("b"))),
            (Some("b".to_owned()), page(&[2, 3], None)),
        ] {
            while server.calls().last() != Some(&key) {
                tokio::task::yield_now().await;
            }
            tokio::task::yield_now().await;
            server.open(&key, reply);
        }
    };
    let (items, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(consumer, driver)
    })
    .await
    .expect("a lost wakeup would hang here");
    assert_eq!(items, [1, 2, 3]);
}
