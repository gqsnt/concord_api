//! Deterministic engine accounting, scheduling and deadline tests.

use concord_retry::{Decision, Policy, Replay, Retry, StopReason};
use std::{
    cell::{Cell, RefCell},
    future::{pending, ready},
    num::NonZeroUsize,
    time::Duration,
};

fn policy(attempts: usize) -> Policy {
    Policy {
        initial_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(8),
        ..Policy::new(NonZeroUsize::new(attempts).unwrap())
    }
}

#[tokio::test]
async fn count_includes_first_and_factory_called_exactly_once_per_attempt() {
    let calls = RefCell::new(Vec::new());
    let clock = Cell::new(Duration::ZERO);
    let result = Retry::new(policy(4), Replay::Permitted)
        .run(
            |n| {
                calls.borrow_mut().push(n);
                ready(n)
            },
            |_| Decision::RetryAfter(None),
            || clock.get(),
            |d| {
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(*calls.borrow(), [1, 2, 3, 4]);
    assert_eq!(result.outcome, Some(4));
    assert_eq!(result.report.attempts, 4);
    assert_eq!(result.report.stop, StopReason::Attempts);
    assert_eq!(result.report.waits, [1, 2, 4].map(Duration::from_secs));
    assert_eq!(result.report.total_waited, Duration::from_secs(7));
}

#[tokio::test]
async fn one_attempt_never_waits_or_calls_jitter() {
    let result = Retry::new(policy(1), Replay::Permitted)
        .run(
            ready,
            |_| Decision::RetryAfter(None),
            || Duration::ZERO,
            |_| async { panic!("no wait") },
            |_, _| panic!("no jitter"),
        )
        .await;
    assert_eq!(result.report.attempts, 1);
    assert_eq!(result.report.stop, StopReason::Attempts);
    assert!(result.report.waits.is_empty());
}

#[tokio::test]
async fn done_and_classifier_stop_precede_replay_and_count() {
    for (decision, stop) in [
        (Decision::Done, StopReason::Done),
        (Decision::Stop, StopReason::ClassifierStop),
    ] {
        let result = Retry::new(policy(1), Replay::Never)
            .run(
                |_| ready("secret"),
                |_| decision,
                || Duration::ZERO,
                |_| async { panic!("no wait") },
                |_, _| panic!("no jitter"),
            )
            .await;
        assert_eq!(result.report.stop, stop);
        assert_eq!(result.outcome, Some("secret"));
        assert!(!format!("{result:?}").contains("secret"));
    }
}

#[tokio::test]
async fn no_permission_even_for_recreatable_outcomes() {
    let result = Retry::new(policy(3), Replay::Never)
        .run(
            ready,
            |_| Decision::RetryAfter(None),
            || Duration::ZERO,
            |_| async { panic!("no wait") },
            |_, _| panic!("no jitter"),
        )
        .await;
    assert_eq!(result.report.stop, StopReason::NotReplayable);
    assert!(!result.report.replay_available);
    assert_eq!(result.report.attempts, 1);
}

#[tokio::test]
async fn deadline_rejects_server_minimum_without_shortening_or_sleeping() {
    for minimum in [3, 4, u64::MAX] {
        let p = Policy {
            timeout: Some(Duration::from_secs(3)),
            ..policy(10)
        };
        let result = Retry::new(p, Replay::Permitted)
            .run(
                |_| ready(503),
                |_| Decision::RetryAfter(Some(Duration::from_secs(minimum))),
                || Duration::ZERO,
                |_| async { panic!("cannot sleep then retry early") },
                |_, _| Duration::ZERO,
            )
            .await;
        assert_eq!(result.report.stop, StopReason::Deadline);
        assert_eq!(result.report.attempts, 1);
        assert!(result.report.waits.is_empty());
        assert_eq!(result.outcome, Some(503));
    }
}

#[tokio::test]
async fn one_deadline_includes_attempts_classification_and_waits() {
    let clock = Cell::new(Duration::from_secs(100));
    let p = Policy {
        timeout: Some(Duration::from_secs(5)),
        ..policy(10)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |n| {
                clock.set(clock.get() + Duration::from_secs(1));
                ready(n)
            },
            |_| Decision::RetryAfter(None),
            || clock.get(),
            |d| {
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.attempts, 2);
    assert_eq!(result.report.waits, [Duration::from_secs(1)]);
    assert_eq!(result.report.stop, StopReason::Deadline);
    assert_eq!(result.outcome, Some(2));
}

#[tokio::test]
async fn oversleep_never_starts_another_attempt_and_is_accounted() {
    let clock = Cell::new(Duration::ZERO);
    let p = Policy {
        timeout: Some(Duration::from_secs(3)),
        ..policy(10)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            ready,
            |_| Decision::RetryAfter(None),
            || clock.get(),
            |_| {
                clock.set(Duration::from_secs(4));
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.attempts, 1);
    assert_eq!(result.report.total_waited, Duration::from_secs(4));
    assert_eq!(result.report.stop, StopReason::Deadline);
}

#[tokio::test]
async fn zero_deadline_starts_nothing() {
    let p = Policy {
        timeout: Some(Duration::ZERO),
        ..policy(10)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |_| async { panic!("no factory call") },
            |_| Decision::Done,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.attempts, 0);
    assert_eq!(result.outcome, None);
    assert_eq!(result.report.stop, StopReason::Deadline);
}

#[tokio::test]
async fn jitter_is_bounded_and_cannot_reduce_server_minimum() {
    for jitter in [Duration::ZERO, Duration::from_millis(500), Duration::MAX] {
        let clock = Cell::new(Duration::ZERO);
        let result = Retry::new(policy(2), Replay::Permitted)
            .run(
                ready,
                |_| Decision::RetryAfter(Some(Duration::from_secs(2))),
                || clock.get(),
                |d| {
                    clock.set(clock.get() + d);
                    ready(())
                },
                |_, ceiling| {
                    assert_eq!(ceiling, Duration::from_secs(1));
                    jitter
                },
            )
            .await;
        assert_eq!(result.report.waits, [Duration::from_secs(2)]);
    }
}

#[tokio::test]
async fn local_jitter_clamps_to_ceiling_including_zero() {
    for (jitter, expected) in [
        (Duration::ZERO, 0),
        (Duration::from_secs(1), 1),
        (Duration::MAX, 1),
    ] {
        let result = Retry::new(policy(2), Replay::Permitted)
            .run(
                ready,
                |_| Decision::RetryAfter(None),
                || Duration::ZERO,
                |_| ready(()),
                |_, _| jitter,
            )
            .await;
        assert_eq!(
            result.report.waits,
            if expected == 0 {
                vec![]
            } else {
                vec![Duration::from_secs(expected)]
            }
        );
    }
}

#[test]
fn exponential_is_capped_and_overflow_safe_for_huge_attempt_numbers() {
    let p = policy(1);
    assert_eq!(
        [0, 1, 2, 3, 4, 5, usize::MAX].map(|n| p.backoff(n)),
        [1, 1, 2, 4, 8, 8, 8].map(Duration::from_secs)
    );
    let p = Policy {
        initial_delay: Duration::from_nanos(1),
        max_delay: Duration::MAX,
        ..p
    };
    assert_eq!(p.backoff(64), Duration::from_nanos(1u64 << 63));
    assert_eq!(p.backoff(usize::MAX), Duration::MAX);
    let p = Policy {
        initial_delay: Duration::MAX,
        ..p
    };
    assert_eq!(p.backoff(2), Duration::MAX);
    let p = Policy {
        initial_delay: Duration::ZERO,
        ..p
    };
    assert_eq!(p.backoff(usize::MAX), Duration::ZERO);
}

#[tokio::test]
async fn pending_attempt_is_cancelled_by_total_deadline() {
    let dropped = Cell::new(false);
    struct Guard<'a>(&'a Cell<bool>);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let p = Policy {
        timeout: Some(Duration::from_secs(5)),
        ..policy(2)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |_| {
                let guard = Guard(&dropped);
                async move {
                    let _guard = guard;
                    pending::<u8>().await
                }
            },
            |_| Decision::Done,
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert!(dropped.get());
    assert_eq!(result.outcome, None);
    assert_eq!(result.report.stop, StopReason::Deadline);
    assert_eq!(result.report.attempts, 1);
}

#[tokio::test]
async fn timeout_of_later_attempt_retains_last_completed_outcome() {
    let p = Policy {
        timeout: Some(Duration::from_secs(5)),
        initial_delay: Duration::ZERO,
        ..policy(3)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |n| async move { if n == 1 { 503 } else { pending().await } },
            |_| Decision::RetryAfter(None),
            || Duration::ZERO,
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.outcome, Some(503));
    assert_eq!(result.report.attempts, 2);
    assert_eq!(result.report.stop, StopReason::Deadline);
}

#[tokio::test]
async fn classification_time_and_synchronous_factory_cost_count() {
    let clock = Cell::new(Duration::ZERO);
    let p = Policy {
        timeout: Some(Duration::from_secs(1)),
        ..policy(2)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |_| ready(200),
            |_| {
                clock.set(Duration::from_secs(1));
                Decision::Done
            },
            || clock.get(),
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.stop, StopReason::Deadline);
    assert_eq!(result.outcome, Some(200));
    clock.set(Duration::ZERO);
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |_| {
                clock.set(Duration::from_secs(1));
                ready(200)
            },
            |_| Decision::Done,
            || clock.get(),
            |_| ready(()),
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(result.report.stop, StopReason::Deadline);
    assert_eq!(result.report.attempts, 1);
    assert_eq!(result.outcome, None);
}

#[cfg(feature = "tokio")]
#[tokio::test]
async fn tokio_convenience_executes_without_a_real_wait() {
    let result = Retry::new(policy(1), Replay::Permitted)
        .run_tokio(|_| ready(200), |_| Decision::Done, |_, ceiling| ceiling)
        .await;
    assert_eq!(result.outcome, Some(200));
    assert_eq!(result.report.stop, StopReason::Done);
}

#[tokio::test]
async fn pending_attempt_timer_uses_budget_remaining_after_prior_attempt_and_wait() {
    let clock = Cell::new(Duration::ZERO);
    let sleeps = RefCell::new(Vec::new());
    let p = Policy {
        timeout: Some(Duration::from_secs(5)),
        ..policy(3)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |n| {
                if n == 1 {
                    clock.set(Duration::from_secs(2));
                }
                async move { if n == 1 { 503 } else { pending().await } }
            },
            |_| Decision::RetryAfter(None),
            || clock.get(),
            |d| {
                sleeps.borrow_mut().push(d);
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(*sleeps.borrow(), [1, 2].map(Duration::from_secs));
    assert_eq!(result.report.waits, [Duration::from_secs(1)]);
    assert_eq!(result.report.total_waited, Duration::from_secs(1));
    assert_eq!(clock.get(), Duration::from_secs(5));
    assert_eq!(result.report.attempts, 2);
    assert_eq!(result.outcome, Some(503));
    assert_eq!(result.report.stop, StopReason::Deadline);
}

#[tokio::test]
async fn time_inside_first_poll_is_subtracted_before_arming_deadline_timer() {
    let clock = Cell::new(Duration::ZERO);
    let p = Policy {
        timeout: Some(Duration::from_secs(5)),
        ..policy(2)
    };
    let result = Retry::new(p, Replay::Permitted)
        .run(
            |_| {
                std::future::poll_fn::<u8, _>(|_| {
                    clock.set(Duration::from_secs(2));
                    std::task::Poll::Pending
                })
            },
            |_| Decision::Done,
            || clock.get(),
            |d| {
                assert_eq!(d, Duration::from_secs(3));
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(clock.get(), Duration::from_secs(5));
    assert_eq!(result.report.stop, StopReason::Deadline);
    assert_eq!(result.report.attempts, 1);
    assert_eq!(result.outcome, None);
}
