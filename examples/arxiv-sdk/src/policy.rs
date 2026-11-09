use crate::{ApiFailure, Atom, ERROR_BODY_LIMIT, Error, Feed, HttpFailure};
use concord::{Endpoint, SendWith};
use concord_ratelimit::{Budget, Clock, Limiter, Mode, TokioClock};
use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

/// Shared, serial dispatch with at least three seconds between admissions; no retry.
/// Clones share pacing. Independent policies do not coordinate across clients or processes.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct Policy(Limiter<()>);
impl Default for Policy {
    fn default() -> Self {
        Self::with_clock(TokioClock::default())
    }
}
impl Policy {
    /// Inject a monotonic clock (and its cancellable sleep), for deterministic fixture tests.
    /// Production clocks must honor elapsed time; this cannot disable the three-second budget.
    pub fn with_clock(clock: impl Clock) -> Self {
        let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), clock);
        limiter
            .set_budget(
                (),
                Budget {
                    requests: NonZeroU32::new(1).unwrap(),
                    window: Duration::from_secs(3),
                },
            )
            .expect("one fixed key and positive window");
        limiter
            .set_concurrency((), NonZeroUsize::new(1).unwrap())
            .expect("key already configured");
        Self(limiter)
    }
}

impl SendWith<Feed> for Policy {
    type Output = Result<Feed, Error>;
    async fn send<E: Endpoint<Feed> + Send>(&self, endpoint: E) -> Self::Output {
        let prepared = endpoint.prepare()?;
        let pairs: Vec<_> = prepared.request().url().query_pairs().collect();
        let number = |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .and_then(|(_, v)| v.parse::<u64>().ok())
                .ok_or(Error::Input("missing page parameter"))
        };
        let start = number("start")?;
        let size = number("max_results")?;
        if start >= 30_000 || size == 0 || size > 100 || start + size > 30_000 {
            return Err(Error::Input("page exceeds the bounded arXiv search window"));
        }
        // Do not retain URL/query borrows across admission.
        drop(pairs);
        let _lease = self
            .0
            .lease((), Mode::Wait)
            .await
            .map_err(Error::Admission)?;
        self.0
            .acquire(&[()], Mode::Wait)
            .await
            .map_err(Error::Admission)?;
        let received = prepared.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received.document::<Atom, Feed>(ERROR_BODY_LIMIT).await;
            return Err(Error::Http(Box::new(HttpFailure { meta, document })));
        }
        let (mut feed, meta) = received.decode_with_meta().await?;
        if let Some(problem) = feed.problem.take() {
            return Err(Error::Api(Box::new(ApiFailure { meta, problem })));
        }
        if feed.start != start
            || feed.items_per_page == 0
            || feed.items_per_page > size
            || feed.entries.len() as u64 > feed.items_per_page
        {
            return Err(Error::Pagination(
                "response paging fields disagree with request",
            ));
        }
        Ok(feed)
    }
}
