//! Known independent plans (P02, P06, P07, contract §3–4): window bounds with a stalled early
//! page, observed-failure stopping, ordered versus completion order, continue mode, sparse holes,
//! budgets, cancellation and the failure-history storage cost.

mod support;

use std::future::ready;
use std::num::NonZeroUsize;

use concord_pagination::{
    Budget, Keyed, OnError, Options, Order, Outcome, Unfinished, independent,
};
use support::{Counter, Server, Token, now};

fn n(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn options(budget: usize, window: usize) -> Options {
    Options::new(Budget::new(n(budget))).window(n(window))
}

type Reply = Result<String, &'static str>;

fn body(key: u32) -> Reply {
    Ok(format!("page-{key}"))
}

type Emitted<R, E> = Result<Keyed<u32, R>, Keyed<u32, E>>;

fn ok_keys<R, E>(outcomes: &[Emitted<R, E>]) -> Vec<Result<u32, u32>> {
    outcomes
        .iter()
        .map(|outcome| match outcome {
            Ok(page) => Ok(page.key),
            Err(failed) => Err(failed.key),
        })
        .collect()
}

fn states(unfinished: &[Keyed<u32, Unfinished>]) -> Vec<(u32, Unfinished)> {
    unfinished
        .iter()
        .map(|entry| (entry.key, entry.value))
        .collect()
}

#[test]
fn window_one_and_a_bounded_window_return_equivalent_content() {
    let run = |window| {
        let server = Server::<u32, Reply>::new();
        let mut pages = independent(1..=7, options(100, window), |key| server.call(key));
        let mut out = Vec::new();
        let mut opened = Vec::new();
        loop {
            match now(pages.next()) {
                Some(Some(outcome)) => out.push(outcome.unwrap().value),
                Some(None) => break,
                None => {
                    // Release every admitted page, latest first, to invert completion order.
                    for key in server.calls().into_iter().rev() {
                        if !opened.contains(&key) {
                            server.open(&key, body(key));
                            opened.push(key);
                        }
                    }
                }
            }
            assert!(server.peak() <= window);
        }
        (out, server.peak(), pages.finish().outcome)
    };
    let (sequential, peak_one, outcome_one) = run(1);
    let (windowed, peak_three, outcome_three) = run(3);
    assert_eq!(sequential, windowed);
    assert_eq!(sequential.len(), 7);
    assert_eq!((peak_one, peak_three), (1, 3));
    assert_eq!(
        (outcome_one, outcome_three),
        (Outcome::Exhausted, Outcome::Exhausted)
    );
}

#[test]
fn inverted_completion_order_is_ordered_by_default_and_raw_in_completion_mode() {
    for order in [Order::Ordered, Order::Completion] {
        let server = Server::<u32, Reply>::new();
        let mut pages = independent(1..=4, options(10, 4).order(order), |key| server.call(key));
        assert!(now(pages.next()).is_none());
        assert_eq!(server.calls(), [1, 2, 3, 4]);
        let mut emitted = Vec::new();
        for key in [4, 3, 2, 1] {
            server.open(&key, body(key));
            while let Some(Some(outcome)) = now(pages.next()) {
                emitted.push(outcome.unwrap().key);
            }
        }
        match order {
            Order::Ordered => assert_eq!(emitted, [1, 2, 3, 4]),
            Order::Completion => assert_eq!(emitted, [4, 3, 2, 1]),
        }
        assert_eq!(pages.finish().outcome, Outcome::Exhausted);
    }
}

#[test]
fn a_stalled_early_page_bounds_in_flight_plus_retained_results() {
    let server = Server::<u32, Result<(u32, Token), &'static str>>::new();
    let results = Counter::new();
    let mut pages = independent(1..=10, options(100, 3), |key| server.call(key));
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [1, 2, 3]);

    // Page 1 stalls; pages 2 and 3 complete and are retained.
    server.open(&2, Ok((2, results.enter())));
    server.open(&3, Ok((3, results.enter())));
    for _ in 0..5 {
        assert!(now(pages.next()).is_none());
    }
    let progress = pages.progress();
    assert_eq!((progress.in_flight, progress.retained), (1, 2));
    // The retained results occupy their slots: nothing more is admitted behind page 1.
    assert_eq!(server.call_count(), 3);
    assert_eq!(server.active() + results.live(), 3);

    // Page 1 completes: 1, 2, 3 are emitted in order and admission resumes.
    server.open(&1, Ok((1, results.enter())));
    let mut emitted = Vec::new();
    while let Some(Some(outcome)) = now(pages.next()) {
        let page = outcome.unwrap();
        emitted.push(page.value.0); // the result (and its token) is dropped here
    }
    assert_eq!(emitted, [1, 2, 3]);
    assert_eq!(server.calls(), [1, 2, 3, 4, 5, 6]);
    assert_eq!(results.live(), 0);
    assert!(server.peak() <= 3);
    assert!(results.peak() <= 3);
}

/// The contract's required regression: page 1 emitted, page 2 stalled, pages 3–4 completed and
/// retained, page 5 fails.
#[test]
fn a_later_failure_behind_a_stalled_page_reports_failure_five_and_restart_two() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(1..=8, options(100, 4), |key| server.call(key));
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [1, 2, 3, 4]);
    server.open(&1, body(1));
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().key, 1);
    assert!(now(pages.next()).is_none());
    assert_eq!(server.calls(), [1, 2, 3, 4, 5]);
    server.open(&3, body(3));
    server.open(&4, body(4));
    assert!(now(pages.next()).is_none());
    assert_eq!(pages.progress().retained, 2);

    // Page 5 fails while page 2 is still stalled: the failure is returned now, not after 2.
    server.open(&5, Err("page 5 broke"));
    let failed = now(pages.next()).unwrap().unwrap().unwrap_err();
    assert_eq!(
        (failed.key, failed.index, failed.value),
        (5, 4, "page 5 broke")
    );
    // Admission stopped at once and the stalled page's future was dropped.
    assert!(server.dropped(&2));
    assert_eq!(server.active(), 0);
    assert!(now(pages.next()).unwrap().is_none());
    assert_eq!(server.calls(), [1, 2, 3, 4, 5]);

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Failed);
    assert_eq!(report.failed().copied().collect::<Vec<_>>(), [5]);
    assert_eq!(report.restart(), Some(&2));
    assert_eq!(
        states(&report.unfinished),
        [
            (2, Unfinished::Cancelled),
            (3, Unfinished::Retained),
            (4, Unfinished::Retained),
            (5, Unfinished::Failed),
        ]
    );
    let retained: Vec<(u32, String)> = report
        .retained
        .iter()
        .map(|page| (page.key, page.value.clone()))
        .collect();
    assert_eq!(retained, [(3, "page-3".into()), (4, "page-4".into())]);
    assert_eq!(report.progress.emitted, 2);
    assert_eq!(report.progress.succeeded, 1);

    // Reusing the retained results, the work still to run is 2, 5 and the unadmitted rest.
    let (reused, rerun) = report.into_rerun();
    assert_eq!(reused.len(), 2);
    assert_eq!(rerun.collect::<Vec<_>>(), [2, 5, 6, 7, 8]);
}

#[test]
fn no_key_is_admitted_after_an_observed_failure() {
    for order in [Order::Ordered, Order::Completion] {
        let server = Server::<u32, Reply>::new();
        let mut pages = independent(1..=100, options(100, 3).order(order), |key| {
            server.call(key)
        });
        assert!(now(pages.next()).is_none());
        server.open(&3, Err("boom"));
        assert_eq!(now(pages.next()).unwrap().unwrap().unwrap_err().key, 3);
        assert!(server.dropped(&1) && server.dropped(&2));
        for _ in 0..3 {
            assert!(now(pages.next()).unwrap().is_none());
        }
        assert_eq!(server.call_count(), 3);
        let report = pages.finish();
        assert_eq!(report.outcome, Outcome::Failed);
        assert_eq!(report.restart(), Some(&1));
        assert_eq!(
            states(&report.unfinished),
            [
                (1, Unfinished::Cancelled),
                (2, Unfinished::Cancelled),
                (3, Unfinished::Failed)
            ]
        );
        assert_eq!(report.rest.unwrap().next(), Some(4));
    }
}

#[test]
fn continue_mode_in_order_keeps_a_failed_page_behind_the_emission_cursor() {
    let mut pages = independent(1..=3, options(10, 3).on_error(OnError::Continue), |key| {
        ready(if key == 2 {
            Err("page 2 broke")
        } else {
            body(key)
        })
    });
    let mut outcomes = Vec::new();
    while let Some(outcome) = now(pages.next()).unwrap() {
        outcomes.push(outcome);
    }
    assert_eq!(ok_keys(&outcomes), [Ok(1), Err(2), Ok(3)]);
    // The resume report still names page 2 as unfinished unsuccessful work.
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::ExhaustedWithFailures);
    assert!(!report.is_success());
    assert_eq!(report.restart(), Some(&2));
    assert_eq!(states(&report.unfinished), [(2, Unfinished::Failed)]);
    assert_eq!(report.into_rerun().1.collect::<Vec<_>>(), [2]);
}

#[test]
fn continue_mode_with_ordered_output_waits_but_still_bounds_the_window() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(1..=6, options(10, 3).on_error(OnError::Continue), |key| {
        server.call(key)
    });
    assert!(now(pages.next()).is_none());
    server.open(&2, Err("2 broke"));
    server.open(&3, body(3));
    assert!(now(pages.next()).is_none());
    assert_eq!(server.call_count(), 3);
    let checkpoint = pages.checkpoint();
    assert_eq!(
        checkpoint
            .unfinished
            .iter()
            .map(|entry| (entry.key, entry.value))
            .collect::<Vec<_>>(),
        [
            (1, Unfinished::InFlight),
            (2, Unfinished::Failed),
            (3, Unfinished::Retained)
        ]
    );

    // The caller stops: page 1 is dropped, the never-emitted error of 2 is handed back.
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert!(server.dropped(&1));
    assert_eq!(
        states(&report.unfinished),
        [
            (1, Unfinished::Cancelled),
            (2, Unfinished::Failed),
            (3, Unfinished::Retained)
        ]
    );
    assert_eq!(report.unemitted_errors.len(), 1);
    assert_eq!(report.unemitted_errors[0].key, 2);
    assert_eq!(report.unemitted_errors[0].value, "2 broke");
    assert_eq!(report.retained[0].key, 3);
}

#[test]
fn completion_order_describes_sparse_holes() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(
        1..=9,
        options(5, 5)
            .order(Order::Completion)
            .on_error(OnError::Continue),
        |key| server.call(key),
    );
    assert!(now(pages.next()).is_none());
    let mut emitted = Vec::new();
    for (key, reply) in [(3, body(3)), (1, body(1)), (5, Err("5 broke"))] {
        server.open(&key, reply);
        let outcome = now(pages.next()).unwrap().unwrap();
        emitted.push(outcome.map(|page| page.key).map_err(|failed| failed.key));
    }
    assert_eq!(emitted, [Ok(3), Ok(1), Err(5)]);

    // Holes 2 and 4 are in flight, 5 failed; 1 and 3 are done. A single cursor could not say it.
    let checkpoint = pages.checkpoint();
    assert_eq!(checkpoint.admitted, 5);
    assert_eq!(
        checkpoint
            .unfinished
            .iter()
            .map(|entry| (entry.key, entry.value))
            .collect::<Vec<_>>(),
        [
            (2, Unfinished::InFlight),
            (4, Unfinished::InFlight),
            (5, Unfinished::Failed)
        ]
    );
    assert_eq!(checkpoint.restart(), Some(&2));

    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(
        states(&report.unfinished),
        [
            (2, Unfinished::Cancelled),
            (4, Unfinished::Cancelled),
            (5, Unfinished::Failed)
        ]
    );
    // Budget 5 was used: the rest of the plan starts at 6.
    assert_eq!(
        report.into_rerun().1.collect::<Vec<_>>(),
        [2, 4, 5, 6, 7, 8, 9]
    );
}

#[test]
fn the_budget_bounds_an_endless_plan_and_hands_back_the_rest() {
    let mut calls = 0;
    let mut pages = independent(1u32.., options(5, 2), |key| {
        calls += 1;
        ready(body(key))
    });
    let mut emitted = Vec::new();
    while let Some(outcome) = now(pages.next()).unwrap() {
        emitted.push(outcome.unwrap().key);
    }
    assert_eq!(emitted, [1, 2, 3, 4, 5]);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Bounded);
    assert!(report.restart().is_none());
    assert_eq!(report.rest.unwrap().take(2).collect::<Vec<_>>(), [6, 7]);
    assert_eq!(calls, 5);
}

#[test]
fn dropping_the_traversal_drops_in_flight_futures_and_admits_nothing() {
    let server = Server::<u32, Reply>::new();
    {
        let mut pages = independent(1..=100, options(100, 4), |key| server.call(key));
        assert!(now(pages.next()).is_none());
        server.open(&2, body(2));
        assert!(now(pages.next()).is_none());
        assert_eq!(server.active(), 3);
    }
    assert_eq!(server.active(), 0);
    assert!(server.dropped(&1) && server.dropped(&3) && server.dropped(&4));
    assert_eq!(server.call_count(), 4);
}

#[test]
fn finish_stops_admission_and_reports_in_flight_keys_as_cancelled() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(1..=100, options(100, 2), |key| server.call(key));
    assert!(now(pages.next()).is_none());
    server.open(&1, body(1));
    assert_eq!(now(pages.next()).unwrap().unwrap().unwrap().key, 1);
    // The freed slot is refilled by the next poll, not behind the caller's back.
    assert_eq!(server.call_count(), 2);
    let report = pages.finish();
    assert_eq!(report.outcome, Outcome::Stopped);
    assert_eq!(states(&report.unfinished), [(2, Unfinished::Cancelled)]);
    assert_eq!(server.active(), 0);
    assert_eq!(server.call_count(), 2);
    assert_eq!(report.into_rerun().1.take(2).collect::<Vec<_>>(), [2, 3]);
}

#[test]
fn a_cancelled_next_loses_no_result() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(1..=3, options(10, 3).order(Order::Completion), |key| {
        server.call(key)
    });
    assert!(now(pages.next()).is_none());
    server.open(&2, body(2));
    server.open(&3, body(3));
    let mut emitted = Vec::new();
    emitted.push(now(pages.next()).unwrap().unwrap().unwrap().key);
    assert!(now(pages.next()).is_some_and(|outcome| {
        emitted.push(outcome.unwrap().unwrap().key);
        true
    }));
    assert!(now(pages.next()).is_none());
    server.open(&1, body(1));
    emitted.push(now(pages.next()).unwrap().unwrap().unwrap().key);
    emitted.sort();
    assert_eq!(emitted, [1, 2, 3]);
    assert_eq!(server.call_count(), 3);
}

/// Contract §3: the failure history has its own storage cost. It is library-owned, bounded by the
/// budget, or by an explicit failure limit with its own outcome.
#[test]
fn a_long_alternating_sequence_keeps_only_failed_keys_and_a_bounded_window() {
    const KEYS: u32 = 2_000;
    let run = |opts: Options| {
        let mut pages = independent(1..=KEYS, opts, |key| {
            ready(if key % 2 == 0 { Err(key) } else { Ok(key) })
        });
        let (mut ok, mut failed, mut peak) = (0u32, 0u32, 0usize);
        while let Some(outcome) = now(pages.next()).unwrap() {
            match outcome {
                Ok(_) => ok += 1,
                Err(_) => failed += 1,
            }
            let progress = pages.progress();
            peak = peak.max(progress.in_flight + progress.retained);
        }
        (ok, failed, peak, pages.finish())
    };

    let (ok, failed, peak, report) = run(options(KEYS as usize, 4).on_error(OnError::Continue));
    assert_eq!((ok, failed), (KEYS / 2, KEYS / 2));
    assert!(peak <= 4, "window exceeded: {peak}");
    assert_eq!(report.outcome, Outcome::ExhaustedWithFailures);
    // What the library kept: exactly the failed keys, nothing per success.
    assert_eq!(report.unfinished.len(), (KEYS / 2) as usize);
    assert!(
        report
            .unfinished
            .iter()
            .all(|entry| entry.value == Unfinished::Failed && entry.key % 2 == 0)
    );
    assert!(report.retained.is_empty());

    let (ok, failed, peak, report) = run(options(KEYS as usize, 4)
        .on_error(OnError::Continue)
        .failure_limit(n(10)));
    assert_eq!(failed, 10);
    assert!(peak <= 4);
    assert_eq!(report.outcome, Outcome::FailureLimit);
    assert_eq!(report.failed().count(), 10);
    assert!(
        report.progress.admitted <= 20 + 4,
        "admitted {}",
        report.progress.admitted
    );
    assert_eq!(u64::from(ok), report.progress.succeeded);
}

#[tokio::test(flavor = "current_thread")]
async fn real_wakeups_drive_ordered_output_with_inverted_completion() {
    let server = Server::<u32, Reply>::new();
    let mut pages = independent(1..=6, options(10, 3), |key| server.call(key));
    let consumer = async {
        let mut keys = Vec::new();
        while let Some(outcome) = pages.next().await {
            keys.push(outcome.unwrap().key);
        }
        keys
    };
    let driver = async {
        // Release the highest admitted pending key first, with yields in between.
        let mut released = Vec::new();
        while released.len() < 6 {
            tokio::task::yield_now().await;
            let calls = server.calls();
            if let Some(key) = calls.iter().rev().find(|key| !released.contains(*key)) {
                released.push(*key);
                server.open(key, body(*key));
            }
        }
    };
    let (keys, ()) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(consumer, driver)
    })
    .await
    .expect("a lost wakeup would hang here");
    assert_eq!(keys, [1, 2, 3, 4, 5, 6]);
    assert!(server.peak() <= 3);
}
