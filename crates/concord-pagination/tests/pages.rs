//! The ordinary facade (`Pages`, `PageItems`, `ItemStream`): the end is explicit (`Ok(None)` only
//! on provider exhaustion, a cap with a next page or a failure is a typed error, and no call after
//! either returns `Ok(None)`), a pending call can be dropped without losing the page in flight,
//! and the traversal is a nameable, `Send` value over non-`Clone` items, metadata and errors.

mod support;

use std::future::{Future, ready};
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use concord_pagination::{
    Incomplete, ItemCheckpoint, Outcome, Page, PageItems, Pages, PagesError, Resume,
};
use futures_util::StreamExt;
use futures_util::stream::FusedStream;
use support::now;

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// Neither `Clone` nor `Copy`: items are moved out of pages.
#[derive(Debug, PartialEq)]
struct Item(u32);

/// Neither `Clone` nor `Debug`.
struct Meta(&'static str);

/// Neither `Clone` nor `Debug`: the provider's typed error.
struct Failure(&'static str);

type Answer = Result<Page<Item, String, Meta>, Failure>;

fn page(items: &[u32], next: Option<&str>) -> Answer {
    Ok(Page {
        items: items.iter().map(|&value| Item(value)).collect(),
        next: next.map(str::to_owned),
        meta: Meta("meta"),
    })
}

fn values(page: &Page<Item, String, Meta>) -> Vec<u32> {
    page.items.iter().map(|item| item.0).collect()
}

fn at(key: &str) -> Resume<String> {
    Resume::At(key.to_owned())
}

/// Three pages, `None` → `c1` → `c2` → end, with an empty middle page; counts factory calls.
fn three_pages(
    calls: &Arc<AtomicUsize>,
) -> impl FnMut(Option<String>) -> std::future::Ready<Answer> + Send + 'static {
    let calls = Arc::clone(calls);
    move |cursor| {
        calls.fetch_add(1, Ordering::SeqCst);
        ready(match cursor.as_deref() {
            None => page(&[1, 2], Some("c1")),
            Some("c1") => page(&[], Some("c2")),
            _ => page(&[3], None),
        })
    }
}

/// `None` → `k2` → failure; counts factory calls.
fn failing_second_page(
    calls: &Arc<AtomicUsize>,
) -> impl FnMut(Option<String>) -> std::future::Ready<Answer> + Send + 'static {
    let calls = Arc::clone(calls);
    move |cursor| {
        calls.fetch_add(1, Ordering::SeqCst);
        ready(match cursor.as_deref() {
            None => page(&[1, 2], Some("k2")),
            _ => Err(Failure("expired cursor")),
        })
    }
}

fn counter() -> Arc<AtomicUsize> {
    Arc::new(AtomicUsize::new(0))
}

fn count(calls: &Arc<AtomicUsize>) -> usize {
    calls.load(Ordering::SeqCst)
}

#[test]
fn only_provider_exhaustion_is_ok_none_and_empty_pages_continue() {
    let mut pages = Pages::new(Resume::Start, |cursor: Option<String>| {
        ready(match cursor.as_deref() {
            None => page(&[1, 2], Some("c1")),
            Some("c1") => page(&[], Some("c2")), // empty intermediate page: not the end
            Some("c2") => page(&[3], Some("c3")),
            _ => page(&[], None), // empty final page: the end
        })
    });
    let mut seen = Vec::new();
    while let Some(page) = now(pages.next_page()).unwrap().unwrap() {
        seen.push(values(&page));
    }
    assert_eq!(seen, [vec![1, 2], vec![], vec![3], vec![]]);
    // Exhaustion is permanent.
    assert!(now(pages.next_page()).unwrap().unwrap().is_none());
    assert_eq!(pages.checkpoint(), None);
    assert_eq!(pages.progress().admitted, 4);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert!(report.resume.is_none() && report.failed.is_none());

    let calls = counter();
    let mut items = Pages::new(Resume::Start, three_pages(&calls)).items();
    let mut seen = Vec::new();
    while let Some(item) = now(items.next()).unwrap().unwrap() {
        seen.push(item.0);
    }
    assert_eq!(seen, [1, 2, 3]);
    assert!(now(items.next()).unwrap().unwrap().is_none());
    assert_eq!(items.checkpoint(), None);
    let progress = items.progress();
    assert_eq!((progress.items, progress.pages_consumed), (3, 3));
    assert_eq!(count(&calls), 3);
    assert_eq!(items.finish().outcome, Outcome::Exhausted);
}

#[test]
fn a_cap_reached_on_the_final_page_is_exhaustion() {
    let calls = counter();
    let mut pages = Pages::new(Resume::Start, three_pages(&calls)).max_pages(n(3));
    for _ in 0..3 {
        now(pages.next_page()).unwrap().unwrap().unwrap();
    }
    // The third page had no next key: the cap was reached exactly at the end.
    assert!(now(pages.next_page()).unwrap().unwrap().is_none());
    assert!(now(pages.next_page()).unwrap().unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    assert_eq!(count(&calls), 3);

    // The same through the item view and an empty final page.
    let calls = counter();
    let fetch = {
        let calls = Arc::clone(&calls);
        move |cursor: Option<String>| {
            calls.fetch_add(1, Ordering::SeqCst);
            ready(match cursor.as_deref() {
                None => page(&[1], Some("c1")),
                _ => page(&[], None),
            })
        }
    };
    let items = Pages::new(Resume::Start, fetch).max_pages(n(2)).items();
    let collected = items.collect_complete().await_now().unwrap();
    assert_eq!(collected, [Item(1)]);
    assert_eq!(count(&calls), 2);
}

#[test]
fn a_cap_with_a_next_page_is_bounded_with_its_resume_point_and_never_ok_none() {
    let calls = counter();
    let mut pages = Pages::new(Resume::Start, three_pages(&calls)).max_pages(n(2));
    assert_eq!(
        values(&now(pages.next_page()).unwrap().unwrap().unwrap()),
        [1, 2]
    );
    assert_eq!(
        values(&now(pages.next_page()).unwrap().unwrap().unwrap()),
        [] as [u32; 0]
    );

    let Some(Err(PagesError::Bounded {
        resume, progress, ..
    })) = now(pages.next_page())
    else {
        panic!("a cap with a next page is an error");
    };
    assert_eq!(resume, at("c2"));
    assert_eq!((progress.admitted, progress.emitted), (2, 2));
    assert_eq!(count(&calls), 2);

    // Every later call says the traversal already ended; none is `Ok(None)`.
    for _ in 0..3 {
        let Some(Err(error)) = now(pages.next_page()) else {
            panic!("a call after the cap is not the end of the listing");
        };
        let PagesError::Ended {
            outcome,
            resume,
            progress,
            ..
        } = &error
        else {
            panic!("expected Ended, got {error:?}");
        };
        assert_eq!(*outcome, Outcome::Bounded);
        assert_eq!(resume.as_ref(), Some(&at("c2")));
        assert_eq!(progress.admitted, 2);
        assert_eq!(error.outcome(), Outcome::Bounded);
        assert_eq!(error.resume(), Some(&at("c2")));
    }
    assert_eq!(count(&calls), 2);
    assert_eq!(pages.checkpoint(), Some(&at("c2")));

    // The engine's report agrees.
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.resume, Some(at("c2")));
    assert!(report.failed.is_none());
    assert_eq!(report.progress.admitted, 2);

    // A cap counts the requests already made.
    let calls = counter();
    let mut pages = Pages::new(Resume::Start, three_pages(&calls));
    now(pages.next_page()).unwrap().unwrap().unwrap();
    let mut pages = pages.max_pages(n(1));
    let Some(Err(PagesError::Bounded { resume, .. })) = now(pages.next_page()) else {
        panic!("the cap was already used");
    };
    assert_eq!(resume, at("c1"));
    assert_eq!(count(&calls), 1);
}

#[test]
fn a_provider_error_is_moved_typed_and_names_the_failed_page() {
    let calls = counter();
    let mut pages = Pages::new(Resume::Start, failing_second_page(&calls));
    now(pages.next_page()).unwrap().unwrap().unwrap();

    let Some(Err(error)) = now(pages.next_page()) else {
        panic!("the failure is returned");
    };
    assert_eq!(error.outcome(), Outcome::Failed);
    assert_eq!(error.resume(), Some(&at("k2")));
    let PagesError::Provider {
        error: failure,
        failed,
        progress,
        ..
    } = error
    else {
        panic!("a provider failure");
    };
    // The factory's own value, moved (it has neither `Clone` nor `Debug`).
    assert_eq!(failure.0, "expired cursor");
    assert_eq!(failed, at("k2"));
    assert_eq!((progress.admitted, progress.failed), (2, 1));

    for _ in 0..3 {
        let Some(Err(PagesError::Ended {
            outcome, resume, ..
        })) = now(pages.next_page())
        else {
            panic!("a call after a failure is not the end of the listing");
        };
        assert_eq!(outcome, Outcome::Failed);
        assert_eq!(resume, Some(at("k2")));
    }
    // No continuation is guessed.
    assert_eq!(count(&calls), 2);

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(at("k2")));
    assert_eq!(report.resume, Some(at("k2")));

    // An ended traversal stays ended through the item view.
    let calls = counter();
    let mut pages = Pages::new(Resume::Start, failing_second_page(&calls));
    now(pages.next_page()).unwrap().unwrap().unwrap();
    assert!(matches!(
        now(pages.next_page()),
        Some(Err(PagesError::Provider { .. }))
    ));
    let mut items = pages.items();
    assert!(matches!(
        now(items.next()),
        Some(Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        }))
    ));
    assert_eq!(count(&calls), 2);
}

#[test]
fn items_follow_the_same_end_rules() {
    // A cap with a next page.
    let calls = counter();
    let mut items = Pages::new(Resume::Start, three_pages(&calls))
        .items()
        .max_pages(n(2));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(1)));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(2)));
    let Some(Err(PagesError::Bounded {
        resume, progress, ..
    })) = now(items.next())
    else {
        panic!("a cap with a next page is an error");
    };
    assert_eq!(resume, at("c2"));
    assert_eq!(progress.admitted, 2);
    assert_eq!(
        items.checkpoint(),
        Some(&ItemCheckpoint {
            page: at("c2"),
            skip: 0
        })
    );
    for _ in 0..2 {
        assert!(matches!(
            now(items.next()),
            Some(Err(PagesError::Ended {
                outcome: Outcome::Bounded,
                ..
            }))
        ));
    }
    assert_eq!(count(&calls), 2);
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.progress.items, 2);

    // A failure.
    let calls = counter();
    let mut items = Pages::new(Resume::Start, failing_second_page(&calls)).items();
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(1)));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(2)));
    let Some(Err(PagesError::Provider { error, failed, .. })) = now(items.next()) else {
        panic!("the page failure is returned");
    };
    assert_eq!((error.0, failed), ("expired cursor", at("k2")));
    assert!(matches!(
        now(items.next()),
        Some(Err(PagesError::Ended {
            outcome: Outcome::Failed,
            ..
        }))
    ));
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed, Some(at("k2")));
    assert_eq!(count(&calls), 2);
}

#[test]
fn collect_complete_keeps_the_prefix_on_a_cap_and_on_a_failure() {
    // Pages, cap.
    let calls = counter();
    let Err(incomplete) = Pages::new(Resume::Start, three_pages(&calls))
        .max_pages(n(2))
        .collect_complete()
        .await_now()
    else {
        panic!("a capped listing is not complete");
    };
    let Incomplete {
        collected, error, ..
    } = incomplete;
    assert_eq!(
        collected.iter().map(values).collect::<Vec<_>>(),
        [vec![1, 2], vec![]]
    );
    assert!(matches!(&error, PagesError::Bounded { resume, .. } if *resume == at("c2")));

    // Pages, failure.
    let calls = counter();
    let Err(incomplete) = Pages::new(Resume::Start, failing_second_page(&calls))
        .collect_complete()
        .await_now()
    else {
        panic!("a failed listing is not complete");
    };
    assert_eq!(incomplete.collected.len(), 1);
    assert!(matches!(
        &incomplete.error,
        PagesError::Provider { error, failed, .. } if error.0 == "expired cursor" && *failed == at("k2")
    ));

    // Items, cap and failure.
    let calls = counter();
    let Err(incomplete) = Pages::new(Resume::Start, three_pages(&calls))
        .max_pages(n(1))
        .items()
        .collect_complete()
        .await_now()
    else {
        panic!("a capped listing is not complete");
    };
    assert_eq!(incomplete.collected, [Item(1), Item(2)]);
    assert_eq!(incomplete.error.resume(), Some(&at("c1")));
    let calls = counter();
    let Err(incomplete) = Pages::new(Resume::Start, failing_second_page(&calls))
        .items()
        .collect_complete()
        .await_now()
    else {
        panic!("a failed listing is not complete");
    };
    assert_eq!(incomplete.collected, [Item(1), Item(2)]);
    assert_eq!(incomplete.error.outcome(), Outcome::Failed);

    // Complete listings.
    let calls = counter();
    let pages = Pages::new(Resume::Start, three_pages(&calls))
        .collect_complete()
        .await_now()
        .unwrap();
    assert_eq!(pages.len(), 3);
    assert_eq!(pages[0].meta.0, "meta");
    let calls = counter();
    let items = Pages::new(Resume::Start, three_pages(&calls))
        .items()
        .collect_complete()
        .await_now()
        .unwrap();
    assert_eq!(items, [Item(1), Item(2), Item(3)]);
}

#[test]
fn the_stream_yields_the_terminal_error_then_ends() {
    // A cap: the typed error, then the end.
    let calls = counter();
    let mut stream = Pages::new(Resume::Start, three_pages(&calls))
        .max_pages(n(1))
        .items()
        .into_stream();
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), Item(1));
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), Item(2));
    assert!(!stream.is_terminated());
    let Some(Some(Err(PagesError::Bounded { resume, .. }))) = now(stream.next()) else {
        panic!("the cap is yielded as an error before the end");
    };
    assert_eq!(resume, at("c1"));
    assert!(stream.is_terminated());
    assert!(now(stream.next()).unwrap().is_none());
    assert!(now(stream.next()).unwrap().is_none());
    // The `None` above does not mean exhaustion: the items consumer still says how it ended.
    let mut items = stream.into_inner();
    assert!(matches!(
        now(items.next()),
        Some(Err(PagesError::Ended {
            outcome: Outcome::Bounded,
            ..
        }))
    ));
    assert_eq!(items.finish().outcome, Outcome::Bounded);
    assert_eq!(count(&calls), 1);

    // A failure.
    let calls = counter();
    let mut stream = Pages::new(Resume::Start, failing_second_page(&calls))
        .items()
        .into_stream();
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    while let Some(next) = now(stream.next()).unwrap() {
        match next {
            Ok(item) => ok.push(item.0),
            Err(error) => errors.push(error),
        }
    }
    assert_eq!(ok, [1, 2]);
    assert_eq!(errors.len(), 1);
    assert!(matches!(&errors[0], PagesError::Provider { failed, .. } if *failed == at("k2")));

    // Exhaustion: no error at all.
    let calls = counter();
    let stream = Pages::new(Resume::Start, three_pages(&calls))
        .items()
        .into_stream();
    let all: Vec<_> = stream.collect::<Vec<_>>().await_now();
    assert!(all.iter().all(Result::is_ok));
    assert_eq!(all.len(), 3);
}

#[test]
fn dropping_a_pending_call_keeps_the_page_in_flight_without_a_second_request() {
    let gate = Gate::new();
    let mut pages = Pages::new(Resume::Start, gate.factory());
    assert_eq!(gate.call_count(), 0); // building calls nothing

    // Polled once, then dropped, twice: one request, still owned by the traversal.
    assert!(now(pages.next_page()).is_none());
    assert!(now(pages.next_page()).is_none());
    assert_eq!(gate.calls(), [None]);
    assert!(gate.alive(None));
    assert_eq!(pages.progress().in_flight, 1);
    gate.open(None, page(&[1], Some("c1")));
    assert_eq!(
        values(&now(pages.next_page()).unwrap().unwrap().unwrap()),
        [1]
    );
    assert_eq!(gate.call_count(), 1);

    // The item view, and no read-ahead: the next page is requested only when asked for.
    let mut items = pages.items();
    assert!(now(items.next()).is_none());
    assert!(now(items.next()).is_none());
    assert_eq!(gate.calls(), [None, Some("c1".to_owned())]);
    gate.open(Some("c1"), page(&[2, 3], Some("c2")));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(2)));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(Item(3)));
    assert_eq!(gate.call_count(), 2);

    // The stream.
    let mut stream = items.into_stream();
    assert!(now(stream.next()).is_none());
    assert!(now(stream.next()).is_none());
    assert_eq!(gate.call_count(), 3);
    assert!(gate.alive(Some("c2")));
    gate.open(Some("c2"), page(&[4], None));
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), Item(4));
    assert!(now(stream.next()).unwrap().is_none());
    assert_eq!(gate.call_count(), 3);

    // Finishing (or dropping) drops the page in flight.
    let gate = Gate::<Answer>::new();
    let mut pages = Pages::new(Resume::Start, gate.factory());
    assert!(now(pages.next_page()).is_none());
    assert!(gate.alive(None));
    let report = pages.finish();
    assert!(!gate.alive(None));
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(report.resume, Some(Resume::Start));
    assert_eq!(report.progress.cancelled, 1);
}

#[tokio::test(flavor = "current_thread")]
async fn a_select_that_drops_a_pending_next_keeps_the_request() {
    let gate = Gate::new();
    let mut stream = Pages::new(Resume::Start, gate.factory())
        .items()
        .into_stream();
    tokio::select! {
        biased;
        _ = stream.next() => panic!("nothing was released"),
        () = tokio::task::yield_now() => {}
    }
    assert_eq!(gate.call_count(), 1);
    assert!(gate.alive(None));

    let opener = {
        let gate = gate.clone();
        tokio::spawn(async move {
            tokio::task::yield_now().await;
            gate.open(None, page(&[7], None));
        })
    };
    // A real waker: the next poll resumes the same request.
    assert_eq!(stream.next().await.unwrap().unwrap(), Item(7));
    assert!(stream.next().await.is_none());
    opener.await.unwrap();
    assert_eq!(gate.call_count(), 1);
}

// An SDK's view: a client hands out named traversals.

/// Neither `Clone` nor `Debug`.
struct Invoice {
    number: u32,
}

/// The SDK's cursor: `Clone`, as every key.
#[derive(Clone, PartialEq)]
struct Cursor(u32);

/// Neither `Clone` nor `Debug`.
struct ResponseMeta {
    request: u32,
}

/// Neither `Clone` nor `Debug`.
struct SdkError(String);

impl std::fmt::Display for SdkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone)]
struct Client {
    requests: Arc<AtomicUsize>,
    last: u32,
}

impl Client {
    async fn list(
        &self,
        cursor: Option<Cursor>,
    ) -> Result<Page<Invoice, Cursor, ResponseMeta>, SdkError> {
        // A real suspension point.
        tokio::task::yield_now().await;
        let request = self.requests.fetch_add(1, Ordering::SeqCst) as u32;
        let at = cursor.map_or(0, |cursor| cursor.0);
        if at > self.last {
            return Err(SdkError(format!("cursor {at} expired")));
        }
        Ok(Page {
            items: vec![
                Invoice { number: at * 10 },
                Invoice {
                    number: at * 10 + 1,
                },
            ],
            next: (at < self.last).then_some(Cursor(at + 1)),
            meta: ResponseMeta { request },
        })
    }

    /// What an SDK can now write: a named, returned traversal.
    fn invoices(&self) -> Pages<Invoice, Cursor, ResponseMeta, SdkError> {
        let client = self.clone();
        Pages::new(Resume::Start, move |cursor| {
            let client = client.clone();
            async move { client.list(cursor).await }
        })
    }

    fn invoice_items(&self) -> PageItems<Invoice, Cursor, SdkError> {
        self.invoices().items()
    }
}

fn assert_send<T: Send>(_: &T) {}

#[tokio::test(flavor = "current_thread")]
async fn an_sdk_names_returns_and_moves_the_traversal_across_tasks() {
    let client = Client {
        requests: Arc::new(AtomicUsize::new(0)),
        last: 2,
    };
    let mut pages = client.invoices();
    assert_send(&pages);
    assert_send(&pages.next_page());
    let first = pages.next_page().await.unwrap().unwrap();
    assert_eq!(first.items[1].number, 1);
    assert_eq!(first.meta.request, 0);

    // The rest on another task: the traversal and its futures are `Send + 'static`.
    let rest = tokio::spawn(async move { pages.collect_complete().await });
    let rest = match rest.await.unwrap() {
        Ok(pages) => pages,
        Err(incomplete) => panic!("{incomplete}"),
    };
    let numbers: Vec<u32> = rest
        .iter()
        .flat_map(|page| page.items.iter().map(|invoice| invoice.number))
        .collect();
    assert_eq!(numbers, [10, 11, 20, 21]);

    let mut items = client.invoice_items().into_stream();
    assert_send(&items);
    assert_send(&items.next());
    let mut numbers = Vec::new();
    while let Some(invoice) = items.next().await {
        numbers.push(invoice.unwrap().number);
    }
    assert_eq!(numbers, [0, 1, 10, 11, 20, 21]);
    assert_eq!(client.requests.load(Ordering::SeqCst), 6);

    // A typed failure keeps its own `Display`; the prefix is kept.
    let resumed = Pages::new(Resume::At(Cursor(2)), {
        let client = Client {
            last: 1,
            ..client.clone()
        };
        move |cursor| {
            let client = client.clone();
            async move { client.list(cursor).await }
        }
    });
    let incomplete = resumed.collect_complete().await.unwrap_err();
    assert!(incomplete.collected.is_empty());
    assert_eq!(
        incomplete.to_string(),
        "incomplete after 0 collected: a page request failed (after 0 pages emitted)"
    );
    assert!(matches!(
        incomplete.error,
        PagesError::Provider {
            failed: Resume::At(Cursor(2)),
            ..
        }
    ));
}

#[test]
fn diagnostics_print_counts_and_outcomes_only() {
    const SECRET: &str = "SECRET";
    let secret = |tag: &str| format!("https://example.test/{SECRET}-{tag}?token={SECRET}");
    let assert_clean = |what: &str, text: String| {
        assert!(!text.contains(SECRET), "{what} leaked: {text}");
    };
    // A provider error whose own `Display` quotes its URL, as `std::io::Error` or a raw
    // `reqwest::Error` can: the traversal's diagnostics must not format it.
    #[derive(Debug)]
    struct Fail(String);
    impl std::fmt::Display for Fail {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "request failed: {}", self.0)
        }
    }
    impl std::error::Error for Fail {}

    let fetch = move |cursor: Option<String>| {
        ready(match cursor {
            None => Ok(Page {
                items: vec![secret("item"), secret("item2")],
                next: Some(secret("cursor")),
                meta: secret("meta"),
            }),
            Some(_) => Err(Fail(secret("detail"))),
        })
    };
    let mut pages = Pages::new(Resume::Start, fetch).max_pages(n(5));
    assert_clean("Pages", format!("{pages:?}"));
    now(pages.next_page()).unwrap().unwrap().unwrap();
    assert_clean("Pages", format!("{pages:?}"));
    let Some(Err(error)) = now(pages.next_page()) else {
        panic!("the second page fails");
    };
    assert_clean("PagesError::Provider Debug", format!("{error:?}"));
    assert_clean("PagesError::Provider Display", error.to_string());
    assert_eq!(
        error.to_string(),
        "a page request failed (after 1 pages emitted)"
    );
    assert!(std::error::Error::source(&error).is_some());
    let Some(Err(ended)) = now(pages.next_page()) else {
        panic!("ended");
    };
    assert_clean("PagesError::Ended Debug", format!("{ended:?}"));
    assert_clean("PagesError::Ended Display", ended.to_string());

    let mut items = Pages::new(Resume::Start, fetch).items().max_pages(n(1));
    now(items.next()).unwrap().unwrap().unwrap();
    assert_clean("PageItems", format!("{items:?}"));
    let incomplete = items.collect_complete().await_now().unwrap_err();
    assert_clean("Incomplete Debug", format!("{incomplete:?}"));
    assert_clean("Incomplete Display", incomplete.to_string());
    assert_clean(
        "PagesError::Bounded Debug",
        format!("{:?}", incomplete.error),
    );
    assert_clean("PagesError::Bounded Display", incomplete.error.to_string());

    let mut stream = Pages::new(Resume::Start, fetch).items().into_stream();
    now(stream.next()).unwrap().unwrap().unwrap();
    assert_clean("ItemStream", format!("{stream:?}"));
}

// Test plumbing.

/// Drives a future whose every step is immediately ready.
trait AwaitNow: Future + Sized {
    fn await_now(self) -> Self::Output {
        now(self).expect("every step is ready")
    }
}

impl<F: Future> AwaitNow for F {}

enum Slot<V> {
    Waiting(Option<Waker>),
    Open(V),
    Taken,
}

struct Call<V> {
    key: Option<String>,
    slot: Arc<Mutex<Slot<V>>>,
}

/// A `Send` fake provider whose responses the test releases key by key.
struct Gate<V> {
    calls: Arc<Mutex<Vec<Call<V>>>>,
}

impl<V> Clone for Gate<V> {
    fn clone(&self) -> Self {
        Self {
            calls: Arc::clone(&self.calls),
        }
    }
}

impl<V: Send + 'static> Gate<V> {
    fn new() -> Self {
        Self {
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// The page factory: records the call and returns a future pending until `open`.
    fn factory(&self) -> impl FnMut(Option<String>) -> Held<V> + Send + 'static {
        let gate = self.clone();
        move |key| {
            let slot = Arc::new(Mutex::new(Slot::Waiting(None)));
            gate.calls.lock().unwrap().push(Call {
                key,
                slot: Arc::clone(&slot),
            });
            Held { slot }
        }
    }

    fn last(&self, key: Option<&str>) -> Arc<Mutex<Slot<V>>> {
        let calls = self.calls.lock().unwrap();
        let call = calls
            .iter()
            .rev()
            .find(|call| call.key.as_deref() == key)
            .unwrap_or_else(|| panic!("no call for {key:?}"));
        Arc::clone(&call.slot)
    }

    /// Completes the most recent call for `key` and wakes its poller.
    fn open(&self, key: Option<&str>, value: V) {
        let slot = self.last(key);
        let previous = std::mem::replace(&mut *slot.lock().unwrap(), Slot::Open(value));
        match previous {
            Slot::Waiting(Some(waker)) => waker.wake(),
            Slot::Waiting(None) => {}
            Slot::Open(_) | Slot::Taken => panic!("{key:?} already completed"),
        }
    }

    /// Whether the future of the most recent call for `key` is still owned (not dropped).
    fn alive(&self, key: Option<&str>) -> bool {
        // The gate's record, this temporary, and the future if it is alive.
        Arc::strong_count(&self.last(key)) > 2
    }

    fn calls(&self) -> Vec<Option<String>> {
        let calls = self.calls.lock().unwrap();
        calls.iter().map(|call| call.key.clone()).collect()
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

/// The future returned by [`Gate::factory`].
struct Held<V> {
    slot: Arc<Mutex<Slot<V>>>,
}

impl<V> Future for Held<V> {
    type Output = V;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<V> {
        let mut slot = self.slot.lock().unwrap();
        match std::mem::replace(&mut *slot, Slot::Taken) {
            Slot::Open(value) => Poll::Ready(value),
            Slot::Waiting(_) => {
                *slot = Slot::Waiting(Some(cx.waker().clone()));
                Poll::Pending
            }
            Slot::Taken => panic!("polled after completion"),
        }
    }
}
