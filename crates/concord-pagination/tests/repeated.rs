//! Opt-in refusal is an admission guard, shared by page, item and event views.

mod support;

use std::cell::Cell;
use std::convert::Infallible;
use std::future::{poll_fn, ready};
use std::num::NonZeroUsize;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use concord_pagination::{
    Budget, ItemCheckpoint, Outcome, Page, PageItemEvent, Pages, PagesError, Resume, resume_items,
    sequential,
};
use futures_util::{StreamExt, stream::FusedStream};
use support::now;

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn cycle(from: Resume<String>) -> Pages<u32, String> {
    Pages::new(from, |key: Option<String>| {
        ready(Ok(match key.as_deref() {
            None => Page::new(vec![0], Some("secret-a".into())),
            Some("secret-a") => Page::new(vec![1], Some("secret-b".into())),
            _ => Page::new(vec![2], Some("secret-a".into())),
        }))
    })
}

#[test]
fn immediate_repeat_retains_the_page_and_refuses_before_another_factory_call() {
    let calls = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&calls);
    let mut pages =
        Pages::<_, _, _, Infallible>::new(Resume::At("private-key".to_owned()), move |key| {
            count.fetch_add(1, Ordering::SeqCst);
            ready(Ok(Page::new(vec![42], key)))
        })
        .reject_repeated_keys();
    assert_eq!(
        now(pages.next_page()).unwrap().unwrap().unwrap().items,
        [42]
    );
    let error = now(pages.next_page()).unwrap().unwrap_err();
    assert!(matches!(error, PagesError::RepeatedKey { .. }));
    assert_eq!(error.outcome(), Outcome::RepeatedKey);
    assert_eq!(error.resume(), Some(&Resume::At("private-key".into())));
    assert_eq!(
        (error.progress().admitted, error.progress().emitted),
        (1, 1)
    );
    assert_eq!(error.progress().failed, 0);
    assert!(error.provider_error().is_none());
    assert!(std::error::Error::source(&error).is_none());
    assert!(!format!("{error:?} {error}").contains("private-key"));
    assert!(matches!(
        error.into_provider_error(),
        Err(PagesError::RepeatedKey { .. })
    ));
    assert!(matches!(
        now(pages.next_page()).unwrap(),
        Err(PagesError::Ended {
            outcome: Outcome::RepeatedKey,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::RepeatedKey);
    assert_eq!(report.resume, Some(Resume::At("private-key".into())));
    assert!(report.failed.is_none());
}

#[test]
fn a_longer_cycle_keeps_the_prefix_and_a_new_resume_has_fresh_history() {
    let incomplete = now(cycle(Resume::Start)
        .reject_repeated_keys()
        .items()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    assert_eq!(incomplete.collected, [0, 1, 2]);
    assert_eq!(incomplete.error.progress().admitted, 3);
    assert_eq!(incomplete.error.progress().succeeded, 3);
    assert_eq!(incomplete.error.progress().failed, 0);
    assert!(!format!("{incomplete:?} {incomplete}").contains("secret"));
    let from = incomplete.error.resume().unwrap().clone();
    let mut pages = cycle(from).reject_repeated_keys();
    assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [1]);
    assert_eq!(now(pages.next_page()).unwrap().unwrap().unwrap().items, [2]);
    assert!(matches!(
        now(pages.next_page()).unwrap(),
        Err(PagesError::RepeatedKey { .. })
    ));

    // A corrected provider can finish from the same refused position.
    let repaired =
        Pages::<_, _, _, Infallible>::new(incomplete.error.resume().unwrap().clone(), |key| {
            assert_eq!(key.as_deref(), Some("secret-a"));
            ready(Ok(Page::new(vec![3], None)))
        })
        .reject_repeated_keys();
    assert_eq!(
        now(repaired.items().collect_complete()).unwrap().unwrap(),
        [3]
    );
}

#[test]
fn the_cap_precedes_refusal_but_neither_masks_exact_exhaustion() {
    for cap in [1, 2, 3, 4] {
        let incomplete = now(cycle(Resume::Start)
            .max_pages(n(cap))
            .reject_repeated_keys()
            .collect_complete())
        .unwrap()
        .unwrap_err();
        assert_eq!(incomplete.collected.len(), cap.min(3));
        assert_eq!(
            incomplete.error.outcome(),
            if cap <= 3 {
                Outcome::Bounded
            } else {
                Outcome::RepeatedKey
            }
        );
        assert_eq!(incomplete.error.progress().admitted, cap.min(3) as u64);
    }
    let final_page = Pages::<_, String, _, Infallible>::new(Resume::Start, |_| {
        ready(Ok(Page::new(vec![7], None)))
    })
    .reject_repeated_keys()
    .max_pages(n(1));
    assert_eq!(
        now(final_page.items().collect_complete()).unwrap().unwrap(),
        [7]
    );
}

#[test]
fn guard_history_survives_conversion_and_reenabling_and_streams_yield_the_error() {
    let mut pages = cycle(Resume::Start).reject_repeated_keys();
    for expected in [0, 1] {
        assert_eq!(
            now(pages.next_page()).unwrap().unwrap().unwrap().items,
            [expected]
        );
    }
    let mut stream = pages.reject_repeated_keys().items().into_stream();
    assert_eq!(now(stream.next()).unwrap().unwrap().unwrap(), 2);
    assert!(matches!(
        now(stream.next()).unwrap(),
        Some(Err(PagesError::RepeatedKey { .. }))
    ));
    assert!(stream.is_terminated());
    assert!(now(stream.next()).unwrap().is_none());
    assert_eq!(stream.into_inner().finish().outcome, Outcome::RepeatedKey);

    // The item entry point can opt in before its first admission too.
    let incomplete = now(cycle(Resume::Start)
        .items()
        .reject_repeated_keys()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    assert_eq!(incomplete.collected, [0, 1, 2]);
    assert_eq!(incomplete.error.outcome(), Outcome::RepeatedKey);
}

#[test]
fn events_emit_the_prefix_and_page_metadata_without_a_false_complete() {
    let mut events = cycle(Resume::Start).reject_repeated_keys().item_events();
    for expected in [0, 1, 2] {
        assert!(
            matches!(now(events.next()).unwrap().unwrap(), Some(PageItemEvent::Item(n)) if n == expected)
        );
        assert!(matches!(
            now(events.next()).unwrap().unwrap(),
            Some(PageItemEvent::Page(()))
        ));
    }
    assert!(matches!(
        now(events.next()).unwrap(),
        Err(PagesError::RepeatedKey { .. })
    ));
    let report = events.finish();
    assert_eq!(report.outcome, Outcome::RepeatedKey);
    assert_eq!(
        (report.progress.items, report.progress.pages_consumed),
        (3, 3)
    );
    assert_eq!(report.resume.unwrap().page, Resume::At("secret-a".into()));
}

#[test]
fn engine_items_and_resumed_items_support_non_send_borrowed_keys() {
    let text = "borrowed".to_owned();
    let key = Rc::new(text.as_str());
    let budget = Budget::new(n(5));
    let mut pages = sequential(Resume::At(key.clone()), budget, |key| {
        ready(Ok::<_, Infallible>(Page::new(vec![1, 2], key)))
    })
    .reject_repeated_keys();
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().items, [1, 2]);
    assert!(now(pages.next()).unwrap().is_none());
    assert_eq!(pages.finish().outcome, Outcome::RepeatedKey);

    let at = ItemCheckpoint {
        page: Resume::At(key),
        skip: 1,
    };
    let mut items = resume_items(at, budget, |key| {
        ready(Ok::<_, Infallible>(Page::new(vec![1, 2], key)))
    })
    .reject_repeated_keys();
    assert_eq!(now(items.next()).unwrap().unwrap().unwrap(), 2);
    assert!(now(items.next()).unwrap().is_none());
    let report = items.finish();
    assert_eq!(report.outcome, Outcome::RepeatedKey);
    assert_eq!(report.progress.pages.admitted, 1);
    assert!(report.failed.is_none());
}

#[test]
fn cancelling_a_pending_next_does_not_readmit_or_refuse_the_in_flight_key() {
    let complete = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let mut pages = sequential(Resume::At("key"), Budget::new(n(5)), {
        let (complete, calls) = (complete.clone(), calls.clone());
        move |key| {
            calls.set(calls.get() + 1);
            let complete = complete.clone();
            poll_fn(move |_| {
                if complete.get() {
                    Poll::Ready(Ok::<_, Infallible>(Page::new(vec![1], key)))
                } else {
                    Poll::Pending
                }
            })
        }
    })
    .reject_repeated_keys();
    assert!(now(pages.next()).is_none());
    assert!(now(pages.next()).is_none());
    assert_eq!(calls.get(), 1);
    complete.set(true);
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().items, [1]);
    assert!(now(pages.next()).unwrap().is_none());
    assert_eq!(calls.get(), 1);
    assert_eq!(pages.finish().outcome, Outcome::RepeatedKey);
}

#[test]
fn default_needs_no_equality_or_hashing_and_adds_no_key_clones() {
    // Intentionally no Eq, Hash or Debug implementation.
    struct Key(Arc<AtomicUsize>);
    impl Clone for Key {
        fn clone(&self) -> Self {
            self.0.fetch_add(1, Ordering::SeqCst);
            Self(Arc::clone(&self.0))
        }
    }
    let clones = Arc::new(AtomicUsize::new(0));
    let next = Arc::clone(&clones);
    let mut calls = 0;
    let pages =
        Pages::<_, _, _, Infallible>::new(Resume::At(Key(Arc::clone(&clones))), move |_| {
            calls += 1;
            ready(Ok(Page::new(
                vec![calls],
                (calls == 1).then(|| Key(Arc::clone(&next))),
            )))
        });
    assert_eq!(now(pages.collect_complete()).unwrap().unwrap().len(), 2);
    // Existing engine cost: two factory arguments and one returned continuation copy.
    assert_eq!(clones.load(Ordering::SeqCst), 3);

    // Defaults still follow repeats until a configured cap.
    let incomplete = now(cycle(Resume::Start)
        .max_pages(n(5))
        .items()
        .collect_complete())
    .unwrap()
    .unwrap_err();
    assert_eq!(incomplete.collected, [0, 1, 2, 1, 2]);
    assert_eq!(incomplete.error.outcome(), Outcome::Bounded);
}
