use bytes::Bytes;
use concord_core::advanced::{
    RateLimitContext, RateLimitFuture, RateLimitPermit, RateLimitResponseAction,
    RateLimitResponseContext, RateLimiter,
};
use concord_examples::policy_stack::PolicyApi;
use concord_test_support::{MockReply, mock};
use http::header::RETRY_AFTER;
use http::{HeaderValue, StatusCode};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::test]
async fn retry_profile_honors_max_attempts() {
    let (transport, handle) = mock()
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .reply(MockReply::status(StatusCode::INTERNAL_SERVER_ERROR))
        .build();
    let api = PolicyApi::new_with_transport(transport);

    let err = api
        .retry_only()
        .execute()
        .await
        .expect_err("max_attempts=2 should stop after two 500s");

    assert_eq!(err.http_status(), Some(StatusCode::INTERNAL_SERVER_ERROR));
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn retry_after_handled_by_rate_limiter_does_not_sleep_twice() {
    let limiter = Arc::new(RecordingLimiter::limited_with_cooldown(
        Duration::from_secs(5),
    ));
    let (transport, handle) = mock()
        .reply(
            MockReply::status(StatusCode::TOO_MANY_REQUESTS)
                .with_header(RETRY_AFTER, HeaderValue::from_static("5")),
        )
        .reply(MockReply::ok_text(Bytes::from_static(b"ok")))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });

    let value = tokio::time::timeout(Duration::from_millis(250), api.rate_limited().execute())
        .await
        .expect("rate limiter handled retry-after; client must not sleep for header duration")
        .expect("retry after 429 succeeds");

    assert_eq!(value, "ok");
    assert_eq!(
        limiter.events(),
        vec![
            "rate_acquire",
            "rate_response:429",
            "rate_acquire",
            "rate_response:200",
        ]
    );
    handle.assert_recorded_len(2);
    handle.finish();
}

#[tokio::test]
async fn rate_limit_limiter_observes_successful_response() {
    let limiter = Arc::new(RecordingLimiter::default());
    let (transport, handle) = mock()
        .reply(MockReply::ok_text(Bytes::from_static(b"limited-ok")))
        .build();
    let mut api = PolicyApi::new_with_transport(transport);
    api.configure_mut(|cfg| {
        cfg.rate_limiter(limiter.clone());
    });

    let value = api.rate_limited().execute().await.unwrap();

    assert_eq!(value, "limited-ok");
    assert_eq!(limiter.events(), vec!["rate_acquire", "rate_response:200"]);
    handle.assert_recorded_len(1);
    handle.finish();
}

#[derive(Default)]
struct RecordingLimiter {
    action: Mutex<RateLimitResponseAction>,
    events: Mutex<Vec<String>>,
}

impl RecordingLimiter {
    fn limited_with_cooldown(delay: Duration) -> Self {
        Self {
            action: Mutex::new(RateLimitResponseAction::Limited {
                retry_after: Some(delay),
                target: Default::default(),
                cooldown_stored: true,
            }),
            events: Mutex::new(Vec::new()),
        }
    }

    fn events(&self) -> Vec<String> {
        self.events.lock().expect("limiter events lock").clone()
    }
}

impl RateLimiter for RecordingLimiter {
    fn acquire<'a>(
        &'a self,
        _ctx: RateLimitContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitPermit, concord_core::prelude::ApiClientError>> {
        Box::pin(async move {
            self.events
                .lock()
                .expect("limiter events lock")
                .push("rate_acquire".to_string());
            Ok(RateLimitPermit)
        })
    }

    fn on_response<'a>(
        &'a self,
        ctx: RateLimitResponseContext<'a>,
    ) -> RateLimitFuture<'a, Result<RateLimitResponseAction, concord_core::prelude::ApiClientError>>
    {
        Box::pin(async move {
            self.events
                .lock()
                .expect("limiter events lock")
                .push(format!("rate_response:{}", ctx.status.as_u16()));
            Ok(self.action.lock().expect("limiter action lock").clone())
        })
    }
}
