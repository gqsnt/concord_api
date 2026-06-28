#![allow(dead_code)]

use bytes::Bytes;
use concord_core::advanced::{
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter, Transport, TransportBody, TransportError,
    TransportErrorKind, TransportRequest, TransportResponse,
};
use concord_core::prelude::ApiClientError;
use http::{HeaderMap, StatusCode};
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct EventRecorder {
    events: Arc<Mutex<Vec<String>>>,
}

impl EventRecorder {
    pub fn record(&self, event: impl Into<String>) {
        self.events
            .lock()
            .expect("event recorder poisoned")
            .push(event.into());
    }

    pub fn snapshot(&self) -> Vec<String> {
        self.events.lock().expect("event recorder poisoned").clone()
    }
}

#[derive(Clone, Debug, Default)]
pub struct MockTransport {
    responses: Arc<Mutex<VecDeque<MockResponse>>>,
    pub events: EventRecorder,
}

impl MockTransport {
    pub fn push(&self, response: MockResponse) {
        self.responses
            .lock()
            .expect("mock transport poisoned")
            .push_back(response);
    }

    pub fn next(&self) -> Option<MockResponse> {
        self.responses
            .lock()
            .expect("mock transport poisoned")
            .pop_front()
    }
}

impl Transport for MockTransport {
    fn send(
        &self,
        req: TransportRequest,
    ) -> Pin<Box<dyn Future<Output = Result<TransportResponse, TransportError>> + Send>> {
        let response = self.next();
        let events = self.events.clone();
        Box::pin(async move {
            events.record(format!("transport_send:{}", req.meta.endpoint));
            let response = response.ok_or_else(|| {
                TransportError::with_kind(
                    TransportErrorKind::Other,
                    std::io::Error::other("mock transport exhausted"),
                )
            })?;
            Ok(TransportResponse {
                meta: req.meta,
                url: req.url,
                status: StatusCode::from_u16(response.status).expect("valid mock status"),
                headers: response.headers,
                content_length: Some(response.body.len() as u64),
                rate_limit: req.rate_limit,
                body: Box::new(StaticBody::new(Bytes::from(response.body))),
            })
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MockResponse {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl MockResponse {
    pub fn text(status: u16, body: impl Into<Vec<u8>>) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: body.into(),
        }
    }
}

struct StaticBody {
    next: Option<Bytes>,
}

impl StaticBody {
    fn new(body: Bytes) -> Self {
        Self { next: Some(body) }
    }
}

impl TransportBody for StaticBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<Bytes>, TransportError>> + Send + 'a>> {
        Box::pin(async move { Ok(self.next.take()) })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeRateLimiter {
    pub events: EventRecorder,
}

impl RateLimiter for FakeRateLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.record("rate_limit_acquire");
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        _ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, ApiClientError>> {
        let events = self.events.clone();
        Box::pin(async move {
            events.record("rate_limit_response");
            Ok(RateLimitResponseAction::Continue)
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct FakeAuthProvider {
    pub events: EventRecorder,
}

#[derive(Clone, Debug, Default)]
pub struct DeterministicSleeper {
    pub events: EventRecorder,
}

impl DeterministicSleeper {
    pub async fn sleep(&self, duration: Duration) {
        self.events
            .record(format!("sleep_ms:{}", duration.as_millis()));
    }
}

pub fn assert_event_order(events: &[String], expected: &[&str]) {
    let actual: Vec<&str> = events.iter().map(String::as_str).collect();
    assert_eq!(actual, expected);
}
