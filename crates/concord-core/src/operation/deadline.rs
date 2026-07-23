use std::future::Future;
use std::time::Duration;

/// One monotonic operation budget established at first poll.
#[derive(Clone, Copy)]
pub(crate) struct OperationDeadline {
    started: tokio::time::Instant,
    expires: Option<tokio::time::Instant>,
}

impl OperationDeadline {
    pub(crate) fn new(
        started: tokio::time::Instant,
        timeout: Option<Duration>,
    ) -> Result<Self, DeadlineOverflow> {
        let expires = timeout
            .map(|timeout| started.checked_add(timeout).ok_or(DeadlineOverflow))
            .transpose()?;
        Ok(Self { started, expires })
    }

    pub(crate) const fn expires_at(self) -> Option<tokio::time::Instant> {
        self.expires
    }

    pub(crate) fn remaining(self) -> Option<Duration> {
        self.expires
            .map(|expires| expires.saturating_duration_since(tokio::time::Instant::now()))
    }

    pub(crate) fn elapsed_at(self, fixed_at: tokio::time::Instant) -> Duration {
        fixed_at.saturating_duration_since(self.started)
    }

    pub(crate) fn check(self) -> Result<(), DeadlineElapsed> {
        match self.expires {
            Some(expires) if tokio::time::Instant::now() >= expires => Err(DeadlineElapsed),
            _ => Ok(()),
        }
    }

    pub(crate) async fn run<F>(self, future: F) -> Result<F::Output, DeadlineElapsed>
    where
        F: Future,
    {
        self.check()?;
        let output = match self.expires {
            Some(expires) => tokio::time::timeout_at(expires, future)
                .await
                .map_err(|_| DeadlineElapsed)?,
            None => future.await,
        };
        self.check()?;
        Ok(output)
    }
}

#[derive(Debug)]
pub(crate) struct DeadlineElapsed;
#[derive(Debug)]
pub(crate) struct DeadlineOverflow;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn disabled_deadline_runs_and_elapsed_uses_fixation_instant() {
        let started = tokio::time::Instant::now();
        let deadline = OperationDeadline::new(started, None).expect("disabled deadline");
        assert_eq!(deadline.run(async { 7 }).await.ok(), Some(7));
        let fixed = started + Duration::from_millis(8);
        assert_eq!(deadline.elapsed_at(fixed), Duration::from_millis(8));
    }

    #[tokio::test]
    async fn expired_deadline_fails_before_polling_inner_future() {
        let started = tokio::time::Instant::now();
        let deadline = OperationDeadline::new(started, Some(Duration::ZERO)).expect("deadline");
        let polled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let inner_polled = std::sync::Arc::clone(&polled);
        let result = deadline
            .run(async move {
                inner_polled.store(true, std::sync::atomic::Ordering::SeqCst);
            })
            .await;
        assert!(result.is_err());
        assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
    }
}
