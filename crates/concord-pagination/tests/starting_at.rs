//! Shared traversal starts and repeated-key admission, through every facade consumer.
mod support;

use std::future::{Future, pending, ready};
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use concord_pagination::{Outcome, Page, PageItemEvent, Pages, PagesError, Resume};
use futures_util::{StreamExt, stream::FusedStream};
use support::now;

type Calls = Arc<Mutex<Vec<Option<String>>>>;
type Walk = Pages<u32, String, u32, std::io::Error>;

fn at(key: &str) -> Resume<String> {
    Resume::At(key.into())
}

fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn walk(from: Resume<String>, calls: &Calls, fail: bool) -> Walk {
    let calls = calls.clone();
    Pages::new(from, move |key: Option<String>| {
        calls.lock().unwrap().push(key.clone());
        ready(match key.as_deref() {
            None => Ok(Page {
                items: vec![0],
                next: Some("A".into()),
                meta: 10,
            }),
            Some("A") => Ok(Page {
                items: vec![],
                next: Some("B".into()),
                meta: 11,
            }),
            _ if fail => Err(std::io::Error::other("PROVIDER_CANARY")),
            _ => Ok(Page {
                items: vec![2],
                next: None,
                meta: 12,
            }),
        })
    })
}

#[test]
fn selecting_a_start_matches_the_constructor_with_caps_metadata_errors_and_reports() {
    for from in [Resume::Start, at("A"), at("B")] {
        for fail in [false, true] {
            for bound in [1, 3] {
                let left_calls = Calls::default();
                let right_calls = Calls::default();
                let mut left = walk(from.clone(), &left_calls, fail).max_pages(cap(bound));
                let mut right = walk(Resume::Start, &right_calls, fail)
                    .max_pages(cap(bound))
                    .starting_at(from.clone());
                assert!(left_calls.lock().unwrap().is_empty());
                assert!(right_calls.lock().unwrap().is_empty());
                loop {
                    assert_eq!(left.checkpoint(), right.checkpoint());
                    let a = now(left.next_page()).unwrap();
                    let b = now(right.next_page()).unwrap();
                    match (a, b) {
                        (Ok(Some(a)), Ok(Some(b))) => {
                            assert_eq!((a.items, a.next, a.meta), (b.items, b.next, b.meta));
                        }
                        (Ok(None), Ok(None)) => break,
                        (Err(a), Err(b)) => {
                            assert_eq!(a.outcome(), b.outcome());
                            assert_eq!(a.resume(), b.resume());
                            assert_eq!(a.progress(), b.progress());
                            assert_eq!(
                                a.provider_error().map(|e| e.kind()),
                                b.provider_error().map(|e| e.kind())
                            );
                            break;
                        }
                        other => panic!("different results: {other:?}"),
                    }
                    assert_eq!(left.progress(), right.progress());
                }
                assert_eq!(*left_calls.lock().unwrap(), *right_calls.lock().unwrap());
                let a = left.finish();
                let b = right.finish();
                assert_eq!(
                    (a.outcome, a.progress, a.resume, a.failed),
                    (b.outcome, b.progress, b.resume, b.failed)
                );
            }
        }
    }
}

fn cycle(calls: &Calls) -> Walk {
    let calls = calls.clone();
    Pages::new(Resume::Start, move |key: Option<String>| {
        calls.lock().unwrap().push(key.clone());
        let (items, next, meta) = match key.as_deref() {
            None => (vec![0], "A", 10),
            Some("A") => (vec![], "B", 11),
            _ => (vec![2], "A", 12),
        };
        ready(Ok(Page {
            items,
            next: Some(next.into()),
            meta,
        }))
    })
}

#[test]
fn builders_keep_settings_in_both_orders_and_the_last_start_wins() {
    for order in [false, true] {
        let calls = Calls::default();
        let pages = cycle(&calls).starting_at(at("ignored"));
        let mut pages = if order {
            pages
                .starting_at(at("A"))
                .refuse_repeated_keys()
                .max_pages(cap(2))
        } else {
            pages
                .max_pages(cap(2))
                .refuse_repeated_keys()
                .starting_at(at("A"))
        };
        assert!(calls.lock().unwrap().is_empty());
        assert!(
            now(pages.next_page())
                .unwrap()
                .unwrap()
                .unwrap()
                .items
                .is_empty()
        );
        assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [2]);
        // The cap wins over the repeated A.
        assert!(matches!(
            now(pages.next_page()).unwrap(),
            Err(PagesError::Bounded { .. })
        ));
        assert_eq!(*calls.lock().unwrap(), [Some("A".into()), Some("B".into())]);
        assert_eq!(pages.finish().outcome, Outcome::Bounded);
    }
    let calls = Calls::default();
    let mut pages = cycle(&calls).starting_at(at("A")).refuse_repeated_keys();
    for _ in 0..2 {
        now(pages.next_page()).unwrap().unwrap().unwrap();
    }
    assert!(matches!(
        now(pages.next_page()).unwrap(),
        Err(PagesError::Repeated { .. })
    ));
}

#[test]
fn repeated_keys_stop_before_factory_admission_and_keep_the_full_prefix() {
    let calls = Calls::default();
    let mut pages = cycle(&calls).refuse_repeated_keys();
    for expected in [vec![0], vec![], vec![2]] {
        assert_eq!(
            now(pages.next_page()).unwrap().unwrap().unwrap().items,
            expected
        );
    }
    let before = pages.progress();
    let error = now(pages.next_page()).unwrap().unwrap_err();
    assert!(matches!(error, PagesError::Repeated { .. }));
    assert_eq!(error.outcome(), Outcome::Repeated);
    assert_eq!(error.resume(), Some(&at("A")));
    assert_eq!(error.progress(), before);
    assert!(error.provider_error().is_none());
    assert!(error.into_provider_error().is_err());
    assert_eq!(
        (
            before.admitted,
            before.succeeded,
            before.emitted,
            before.failed
        ),
        (3, 3, 3, 0)
    );
    assert_eq!(
        *calls.lock().unwrap(),
        [None, Some("A".into()), Some("B".into())]
    );
    assert!(matches!(
        now(pages.next_page()).unwrap(),
        Err(PagesError::Ended {
            outcome: Outcome::Repeated,
            ..
        })
    ));
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Repeated);
    assert_eq!(report.resume, Some(at("A")));
    assert!(report.failed.is_none());
    assert_eq!(report.progress, before);
}

#[test]
fn a_resumed_key_is_admitted_once_and_guards_are_traversal_local() {
    for _ in 0..2 {
        let calls = Calls::default();
        let captured = calls.clone();
        let mut pages: Walk = Pages::new(Resume::Start, move |key: Option<String>| {
            captured.lock().unwrap().push(key.clone());
            ready(Ok(Page {
                items: vec![7],
                next: key,
                meta: 1,
            }))
        })
        .refuse_repeated_keys()
        .starting_at(at("A"));
        assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [7]);
        assert!(matches!(
            now(pages.next_page()).unwrap(),
            Err(PagesError::Repeated { .. })
        ));
        assert_eq!(*calls.lock().unwrap(), [Some("A".into())]);
    }
}

#[test]
fn guard_is_optional_and_starting_at_only_requires_a_clone_key() {
    #[derive(Clone)]
    struct Key;
    let mut pages: Pages<(), Key> =
        Pages::new(Resume::Start, |_| ready(Ok(Page::new(vec![], None))))
            .starting_at(Resume::At(Key));
    assert!(now(pages.next_page()).unwrap().unwrap().is_some());
    assert!(now(pages.next_page()).unwrap().unwrap().is_none());
    let calls = Calls::default();
    let incomplete = now(cycle(&calls).max_pages(cap(5)).collect_complete())
        .unwrap()
        .unwrap_err();
    assert_eq!(incomplete.collected.len(), 5);
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
    assert_eq!(calls.lock().unwrap().len(), 5);
}

#[test]
fn a_dropped_unpolled_future_keeps_the_builders_open() {
    let calls = Calls::default();
    let mut pages = walk(Resume::Start, &calls, false);
    drop(pages.next_page());
    let mut pages = pages.starting_at(at("B")).refuse_repeated_keys();
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [2]);
}

#[test]
fn the_first_poll_closes_both_builders_including_pending_and_ended() {
    for guard in [false, true] {
        for state in ["pending", "page", "ended"] {
            let mut pages: Walk = if state == "pending" {
                Pages::new(Resume::Start, |_| pending())
            } else {
                walk(at("B"), &Calls::default(), false)
            };
            {
                let mut next = pin!(pages.next_page());
                let result = next.as_mut().poll(&mut Context::from_waker(Waker::noop()));
                assert_eq!(result.is_pending(), state == "pending");
            }
            if state == "ended" {
                assert!(now(pages.next_page()).unwrap().unwrap().is_none());
            }
            let panic = catch_unwind(AssertUnwindSafe(|| {
                if guard {
                    pages.refuse_repeated_keys()
                } else {
                    pages.starting_at(Resume::Start)
                }
            }))
            .unwrap_err();
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap();
            assert_eq!(
                message,
                if guard {
                    "Pages::refuse_repeated_keys must be called before the first poll"
                } else {
                    "Pages::starting_at must be called before the first poll"
                }
            );
        }
    }
}

#[test]
fn item_collections_streams_and_reports_keep_refusal_and_prefix() {
    let calls = Calls::default();
    let partial = now(cycle(&calls)
        .refuse_repeated_keys()
        .items()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    assert_eq!(partial.collected, [0, 2]);
    assert_eq!(partial.error.outcome(), Outcome::Repeated);
    assert_eq!(partial.error.resume(), Some(&at("A")));
    let mut items = cycle(&Calls::default()).refuse_repeated_keys().items();
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(0));
    assert_eq!(now(items.next()).unwrap().unwrap(), Some(2));
    assert_eq!(
        now(items.next()).unwrap().unwrap_err().outcome(),
        Outcome::Repeated
    );
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::Repeated);
    assert_eq!(report.resume.unwrap().page, at("A"));
    assert!(report.failed.is_none());
    assert_eq!(
        (report.progress.items, report.progress.pages_consumed),
        (2, 3)
    );
    let mut stream = cycle(&Calls::default())
        .refuse_repeated_keys()
        .items()
        .into_stream();
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), 0);
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), 2);
    assert_eq!(
        now(stream.next()).unwrap().unwrap().unwrap_err().outcome(),
        Outcome::Repeated
    );
    assert!(stream.is_terminated());
    assert!(now(stream.next()).unwrap().is_none());
    assert!(now(stream.next()).unwrap().is_none());
}

#[test]
fn events_preserve_empty_page_metadata_and_end_with_refusal() {
    let mut events = cycle(&Calls::default())
        .refuse_repeated_keys()
        .item_events();
    assert!(matches!(
        now(events.next()).unwrap().unwrap(),
        Some(PageItemEvent::Item(0))
    ));
    for meta in [10, 11] {
        assert!(
            matches!(now(events.next()).unwrap().unwrap(), Some(PageItemEvent::Page(m)) if m == meta)
        );
    }
    assert!(matches!(
        now(events.next()).unwrap().unwrap(),
        Some(PageItemEvent::Item(2))
    ));
    assert!(matches!(
        now(events.next()).unwrap().unwrap(),
        Some(PageItemEvent::Page(12))
    ));
    assert_eq!(
        now(events.next()).unwrap().unwrap_err().outcome(),
        Outcome::Repeated
    );
    assert!(matches!(
        now(events.next()).unwrap(),
        Err(PagesError::Ended {
            outcome: Outcome::Repeated,
            ..
        })
    ));
    let report = events.finish();
    assert_eq!(report.outcome, Outcome::Repeated);
    assert_eq!(report.resume.unwrap().page, at("A"));
    assert!(report.failed.is_none());
}

#[test]
fn refusal_formatting_withholds_keys_items_metadata_and_provider_content() {
    #[derive(Clone, Debug, Eq, PartialEq, Hash)]
    struct Secret(&'static str);
    let key = Secret("F1_KEY_CANARY");
    let mut pages: Pages<Secret, Secret, Secret, std::io::Error> =
        Pages::new(Resume::At(key.clone()), move |key: Option<Secret>| {
            ready(Ok(Page {
                items: vec![Secret("F1_ITEM_CANARY")],
                next: key,
                meta: Secret("F1_META_CANARY"),
            }))
        })
        .refuse_repeated_keys();
    let mut shown = format!("{pages:?}");
    now(pages.next_page()).unwrap().unwrap().unwrap();
    let error = now(pages.next_page()).unwrap().unwrap_err();
    assert_eq!(error.resume().unwrap().key(), Some(&key));
    assert!(std::error::Error::source(&error).is_none());
    shown += &format!("{error} {error:?}");
    let ended = now(pages.next_page()).unwrap().unwrap_err();
    shown += &format!("{ended} {ended:?} {:?}", pages.finish());
    let partial = now(walk(Resume::Start, &Calls::default(), true)
        .items()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    shown += &format!("{partial} {partial:?}");
    let partial = now(cycle(&Calls::default())
        .refuse_repeated_keys()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    shown += &format!("{partial} {partial:?}");
    for secret in [
        "F1_KEY_CANARY",
        "F1_ITEM_CANARY",
        "F1_META_CANARY",
        "PROVIDER_CANARY",
    ] {
        assert!(!shown.contains(secret), "{shown}");
    }
}

#[test]
fn a_pending_guarded_request_is_resumed_without_admitting_its_key_twice() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let opened = Arc::new(AtomicBool::new(false));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut pages: Walk = Pages::new(at("A"), {
        let opened = opened.clone();
        let calls = calls.clone();
        move |key: Option<String>| {
            calls.fetch_add(1, Ordering::SeqCst);
            let opened = opened.clone();
            std::future::poll_fn(move |_| {
                if opened.load(Ordering::SeqCst) {
                    Poll::Ready(Ok(Page {
                        items: vec![1],
                        next: key.clone(),
                        meta: 1,
                    }))
                } else {
                    Poll::Pending
                }
            })
        }
    })
    .refuse_repeated_keys();
    assert!(now(pages.next_page()).is_none());
    assert!(now(pages.next_page()).is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    opened.store(true, Ordering::SeqCst);
    assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [1]);
    assert_eq!(
        now(pages.next_page()).unwrap().unwrap_err().outcome(),
        Outcome::Repeated
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn sequential_retains_its_sync_auto_trait() {
    fn require_sync<T: Sync>(_: &T) {}
    let chain = concord_pagination::sequential(Resume::<String>::Start, cap(1).into(), |_| {
        ready(Ok::<_, ()>(Page::<(), String>::new(vec![], None)))
    });
    require_sync(&chain);
}
