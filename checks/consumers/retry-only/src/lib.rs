//! A runtime-free, macro-free consumer, also run against the extracted retry package.

#[cfg(test)]
mod tests {
    use concord_retry::{
        Decision, Policy, Replay, Retry, StopReason,
        http::{RetryAfterError, retry_after},
    };
    use std::{
        cell::Cell,
        future::{Future, ready},
        num::NonZeroUsize,
        pin::pin,
        task::{Context, Poll, Waker},
        time::{Duration, UNIX_EPOCH},
    };

    // All injected futures complete synchronously; no executor or async runtime dependency.
    fn complete<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("fixture should be immediately ready"),
        }
    }

    #[test]
    fn factory_engine_with_injected_time() {
        let clock = Cell::new(Duration::ZERO);
        let policy = Policy {
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(2),
            timeout: Some(Duration::from_secs(10)),
            ..Policy::new(NonZeroUsize::new(3).unwrap())
        };
        let result = complete(Retry::new(policy, Replay::Permitted).run(
            ready,
            |n| {
                if *n < 3 {
                    Decision::RetryAfter(None)
                } else {
                    Decision::Done
                }
            },
            || clock.get(),
            |d| {
                clock.set(clock.get() + d);
                ready(())
            },
            |_, ceiling| ceiling,
        ));
        assert_eq!(result.outcome, Some(3));
        assert_eq!(result.report.attempts, 3);
        assert_eq!(result.report.total_waited, Duration::from_secs(3));
        assert_eq!(result.report.stop, StopReason::Done);
    }

    #[test]
    fn replay_permission_is_explicit() {
        let result = complete(
            Retry::new(Policy::new(NonZeroUsize::new(3).unwrap()), Replay::Never).run(
                |_| ready("private outcome"),
                |_| Decision::RetryAfter(None),
                || Duration::ZERO,
                |_| ready(()),
                |_, ceiling| ceiling,
            ),
        );
        assert_eq!(result.report.attempts, 1);
        assert_eq!(result.report.stop, StopReason::NotReplayable);
        assert!(!format!("{result:?}").contains("private outcome"));
    }

    #[test]
    fn singleton_retry_after_without_an_http_dependency() {
        assert_eq!(
            retry_after([b"7".as_slice()], UNIX_EPOCH),
            Ok(Some(Duration::from_secs(7)))
        );
        assert_eq!(
            retry_after([b"7".as_slice(), b"8"], UNIX_EPOCH),
            Err(RetryAfterError::Repeated)
        );
    }
}
