use crate::transport::TransportError;
use http::{Method, StatusCode};

#[derive(Debug)]
pub enum RetryOutcome<'a> {
    Transport(&'a TransportError),
    HttpStatus(StatusCode),
    Decode,
    Transform,
    Other,
}

#[derive(Debug)]
pub struct RetryContext<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
    pub outcome: RetryOutcome<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RetryDecision {
    Stop,
    Retry,
}

pub trait RetryPolicy: Send + Sync + 'static {
    #[inline]
    fn max_retries(&self) -> u32 {
        0
    }

    #[inline]
    fn should_retry(&self, _ctx: &RetryContext<'_>) -> RetryDecision {
        RetryDecision::Stop
    }
}

#[derive(Default)]
pub struct NoRetryPolicy;

impl RetryPolicy for NoRetryPolicy {}

