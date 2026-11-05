//! First-page discovery (P03, contract §4): the first page runs once, is emitted once and is
//! charged once; the plan is validated and lazy; invalid or huge totals neither allocate nor
//! dispatch unboundedly; a discovery failure leaves no plan.

mod support;

use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{Budget, OnError, Options, Order, Outcome, Unfinished, first_page_then};
use support::{Server, now};

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn options(budget: usize, window: usize) -> Options {
    Options::new(Budget::new(n(budget))).window(n(window))
}

#[derive(Debug, Clone, PartialEq)]
struct Listing {
    total_pages: i64,
    items: Vec<u64>,
}

#[derive(Debug, PartialEq)]
enum ApiError {
    Server(u64),
    Protocol(&'static str),
}

/// The adapter's plan: validate the advertised total, return a lazy range of the other pages.
fn plan(first: &Listing) -> Result<std::ops::RangeInclusive<u64>, ApiError> {
    if first.total_pages < 1 {
        return Err(ApiError::Protocol("total_pages must be at least 1"));
    }
    Ok(2..=first.total_pages as u64)
}

fn listing(total_pages: i64, page: u64) -> Result<Listing, ApiError> {
    Ok(Listing {
        total_pages,
        items: vec![page * 100, page * 100 + 1],
    })
}

#[test]
fn the_first_page_runs_once_is_emitted_once_and_is_charged_once() {
    let server = Server::<u64, Result<Listing, ApiError>>::new();
    let mut pages = first_page_then(1, options(10, 3), |page| server.call(page), plan);

    // The first page is admitted alone, whatever the window.
    assert!(now(pages.next()).is_none());
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [1]);

    server.open(&1, listing(4, 1));
    let first = now(pages.next()).unwrap().unwrap().unwrap();
    assert_eq!(
        (first.index, first.key, first.value.items.clone()),
        (0, 1, vec![100, 101])
    );

    let mut items = first.value.items;
    let mut opened = vec![1];
    loop {
        match now(pages.next()) {
            Some(Some(outcome)) => items.extend(outcome.unwrap().value.items),
            Some(None) => break,
            None => {
                for page in server.calls() {
                    if !opened.contains(&page) {
                        server.open(&page, listing(4, page));
                        opened.push(page);
                    }
                }
            }
        }
    }
    assert_eq!(items, [100, 101, 200, 201, 300, 301, 400, 401]);
    assert_eq!(server.calls().iter().filter(|page| **page == 1).count(), 1);
    assert_eq!(server.calls(), [1, 2, 3, 4]);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Exhausted);
    assert_eq!(report.progress.admitted, 4);
    assert_eq!(report.progress.succeeded, 4);
}

#[test]
fn the_first_page_counts_against_the_page_budget() {
    let mut calls = Vec::new();
    let mut pages = first_page_then(
        1,
        options(3, 2),
        |page| {
            calls.push(page);
            ready(listing(10, page))
        },
        plan,
    );
    let mut keys = Vec::new();
    while let Some(outcome) = now(pages.next()).unwrap() {
        keys.push(outcome.unwrap().key);
    }
    assert_eq!(keys, [1, 2, 3]);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.into_rerun().1.next(), Some(4));
    assert_eq!(calls, [1, 2, 3]);
}

#[test]
fn a_huge_total_neither_allocates_nor_dispatches_unboundedly() {
    let server = Server::<u64, Result<Listing, ApiError>>::new();
    let mut pages = first_page_then(1, options(8, 3), |page| server.call(page), plan);
    assert!(now(pages.next()).is_none());
    // A provider advertising i64::MAX pages: the plan is a range, nothing is collected.
    server.open(&1, listing(i64::MAX, 1));
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().key, 1);
    let mut emitted = 1;
    let mut opened = vec![1];
    loop {
        match now(pages.next()) {
            Some(Some(outcome)) => {
                outcome.unwrap();
                emitted += 1;
            }
            Some(None) => break,
            None => {
                assert!(server.active() <= 3);
                for page in server.calls() {
                    if !opened.contains(&page) {
                        server.open(&page, listing(i64::MAX, page));
                        opened.push(page);
                    }
                }
            }
        }
    }
    assert_eq!(emitted, 8);
    assert_eq!(server.call_count(), 8);
    assert!(server.peak() <= 3);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert_eq!(report.rest.unwrap().next(), Some(9));
}

#[test]
fn an_invalid_total_is_a_discovery_failure_without_a_plan() {
    for on_error in [OnError::Stop, OnError::Continue] {
        let calls = std::cell::Cell::new(0);
        let mut pages = first_page_then(
            1,
            options(100, 4).on_error(on_error),
            |page| {
                calls.set(calls.get() + 1);
                ready(listing(-3, page))
            },
            plan,
        );
        let failed = now(pages.next()).unwrap().unwrap().unwrap_err();
        assert_eq!(
            (failed.index, failed.key, failed.value),
            (0, 1, ApiError::Protocol("total_pages must be at least 1"))
        );
        assert!(now(pages.next()).unwrap().is_none());
        assert_eq!(calls.get(), 1);
        let report = pages.finish();
        assert_eq!(report.outcome, Outcome::DiscoveryFailed);
        assert!(report.outcome.is_discovery_incomplete());
        // The first page is not emitted as a success; there is no remaining plan.
        assert_eq!(report.progress.succeeded, 0);
        assert_eq!(report.progress.failed, 1);
        assert!(report.rest.is_none());
        assert_eq!(report.restart(), Some(&1));
        assert_eq!(report.unfinished[0].value, Unfinished::Failed);
    }
}

#[test]
fn a_first_page_failure_yields_no_plan_in_either_mode() {
    for (on_error, order) in [
        (OnError::Stop, Order::Ordered),
        (OnError::Continue, Order::Ordered),
        (OnError::Continue, Order::Completion),
    ] {
        let calls = std::cell::Cell::new(0);
        let mut pages = first_page_then(
            1,
            options(100, 4).on_error(on_error).order(order),
            |page| {
                calls.set(calls.get() + 1);
                ready(Err::<Listing, _>(ApiError::Server(page)))
            },
            plan,
        );
        let failed = now(pages.next()).unwrap().unwrap().unwrap_err();
        assert_eq!(failed.value, ApiError::Server(1));
        assert!(now(pages.next()).unwrap().is_none());
        assert_eq!(calls.get(), 1);
        let report = pages.finish();
        assert_eq!(report.outcome, Outcome::DiscoveryFailed);
        assert!(report.rest.is_none());
        assert_eq!(report.failed().copied().collect::<Vec<_>>(), [1]);
    }
}

#[test]
fn continue_mode_after_discovery_keeps_keyed_outcomes() {
    let mut pages = first_page_then(
        1,
        options(100, 3).on_error(OnError::Continue),
        |page| {
            ready(if page == 3 {
                Err(ApiError::Server(3))
            } else {
                listing(4, page)
            })
        },
        plan,
    );
    let mut outcomes = Vec::new();
    while let Some(outcome) = now(pages.next()).unwrap() {
        outcomes.push(outcome.map(|page| page.key).map_err(|failed| failed.key));
    }
    assert_eq!(outcomes, [Ok(1), Ok(2), Err(3), Ok(4)]);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::ExhaustedWithFailures);
    assert_eq!(report.restart(), Some(&3));
}

#[test]
fn finishing_before_discovery_reports_stopped_without_a_plan() {
    let server = Server::<u64, Result<Listing, ApiError>>::new();
    let mut pages = first_page_then(1, options(10, 2), |page| server.call(page), plan);
    assert!(now(pages.next()).is_none());
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert!(report.rest.is_none());
    assert!(server.dropped(&1));
    assert_eq!(report.unfinished[0].value, Unfinished::Cancelled);
}
