use bytes::Bytes;

use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EmptyChunkAction {
    Continue,
    YieldExecutor,
    LimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ResponseChunkError {
    Acquisition,
    LimitExceeded,
}

#[derive(Default)]
pub(super) struct ResponseChunkProgress {
    empty_chunks_seen: usize,
    ready_empty_chunks: usize,
}

impl ResponseChunkProgress {
    pub(super) fn begin_turn(&mut self) {
        self.ready_empty_chunks = 0;
    }

    pub(super) fn observe_empty(&mut self) -> EmptyChunkAction {
        let Some(total) = self.empty_chunks_seen.checked_add(1) else {
            return EmptyChunkAction::LimitExceeded;
        };
        if total > MAX_EMPTY_RESPONSE_CHUNKS {
            return EmptyChunkAction::LimitExceeded;
        }
        self.empty_chunks_seen = total;

        let Some(ready) = self.ready_empty_chunks.checked_add(1) else {
            return EmptyChunkAction::LimitExceeded;
        };
        self.ready_empty_chunks = ready;
        if ready == MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN {
            EmptyChunkAction::YieldExecutor
        } else {
            EmptyChunkAction::Continue
        }
    }
}

pub(super) async fn next_nonempty_chunk(
    response: &mut reqwest::Response,
    progress: &mut ResponseChunkProgress,
) -> Result<Option<Bytes>, ResponseChunkError> {
    progress.begin_turn();
    loop {
        let chunk = response.chunk().await.map_err(|error| {
            drop(error);
            ResponseChunkError::Acquisition
        })?;
        match chunk {
            Some(bytes) if bytes.is_empty() => match progress.observe_empty() {
                EmptyChunkAction::Continue => {}
                EmptyChunkAction::YieldExecutor => {
                    tokio::task::yield_now().await;
                    progress.begin_turn();
                }
                EmptyChunkAction::LimitExceeded => {
                    return Err(ResponseChunkError::LimitExceeded);
                }
            },
            chunk => return Ok(chunk),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::collections::VecDeque;
    use std::io;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use futures_core::Stream;

    pub(crate) struct ReadyBody {
        chunks: VecDeque<Result<Bytes, io::Error>>,
        polls: Arc<AtomicUsize>,
        dropped: Arc<AtomicBool>,
    }

    impl Stream for ReadyBody {
        type Item = Result<Bytes, io::Error>;

        fn poll_next(
            mut self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Option<Self::Item>> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Poll::Ready(self.chunks.pop_front())
        }
    }

    impl Drop for ReadyBody {
        fn drop(&mut self) {
            self.dropped.store(true, Ordering::SeqCst);
        }
    }

    pub(crate) fn response(
        chunks: impl IntoIterator<Item = Result<Bytes, io::Error>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
    ) -> reqwest::Response {
        response_with_status(chunks, polls, dropped, http::StatusCode::OK)
    }

    pub(crate) fn response_with_status(
        chunks: impl IntoIterator<Item = Result<Bytes, io::Error>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
        status: http::StatusCode,
    ) -> reqwest::Response {
        let body = reqwest::Body::wrap_stream(ReadyBody {
            chunks: chunks.into_iter().collect(),
            polls: Arc::clone(polls),
            dropped: Arc::clone(dropped),
        });
        let mut response = http::Response::new(body);
        *response.status_mut() = status;
        response.into()
    }

    pub(crate) fn empty_chunks(count: usize) -> Vec<Result<Bytes, io::Error>> {
        std::iter::repeat_with(|| Ok(Bytes::new()))
            .take(count)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_progress_exact_empty_limit_succeeds() {
        let mut progress = ResponseChunkProgress::default();
        for index in 1..=MAX_EMPTY_RESPONSE_CHUNKS {
            let expected = if index % MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN == 0 {
                EmptyChunkAction::YieldExecutor
            } else {
                EmptyChunkAction::Continue
            };
            assert_eq!(progress.observe_empty(), expected);
            if expected == EmptyChunkAction::YieldExecutor {
                progress.begin_turn();
            }
        }
    }

    #[test]
    fn response_progress_first_excess_fails() {
        let mut progress = ResponseChunkProgress::default();
        for _ in 0..MAX_EMPTY_RESPONSE_CHUNKS {
            if progress.observe_empty() == EmptyChunkAction::YieldExecutor {
                progress.begin_turn();
            }
        }
        assert_eq!(progress.observe_empty(), EmptyChunkAction::LimitExceeded);
    }

    #[test]
    fn response_progress_checked_overflow_fails() {
        let mut progress = ResponseChunkProgress {
            empty_chunks_seen: usize::MAX,
            ready_empty_chunks: 0,
        };
        assert_eq!(progress.observe_empty(), EmptyChunkAction::LimitExceeded);
    }

    #[test]
    fn response_progress_total_is_not_reset_by_nonempty_data() {
        let mut progress = ResponseChunkProgress::default();
        for _ in 0..(MAX_EMPTY_RESPONSE_CHUNKS / 2) {
            if progress.observe_empty() == EmptyChunkAction::YieldExecutor {
                progress.begin_turn();
            }
        }
        progress.begin_turn();
        for _ in 0..(MAX_EMPTY_RESPONSE_CHUNKS / 2) {
            if progress.observe_empty() == EmptyChunkAction::YieldExecutor {
                progress.begin_turn();
            }
        }
        assert_eq!(progress.observe_empty(), EmptyChunkAction::LimitExceeded);
    }

    #[test]
    fn all_response_acquisition_modes_share_empty_chunk_limit() {
        let modes = ["streaming", "buffered", "status", "provider"];
        for mode in modes {
            let mut progress = ResponseChunkProgress::default();
            for _ in 0..MAX_EMPTY_RESPONSE_CHUNKS {
                if progress.observe_empty() == EmptyChunkAction::YieldExecutor {
                    progress.begin_turn();
                }
            }
            assert_eq!(
                progress.observe_empty(),
                EmptyChunkAction::LimitExceeded,
                "{mode}"
            );
        }
    }
}
