use std::num::NonZeroUsize;
use std::time::Duration;

use bytes::BytesMut;
use http::Method;

use crate::endpoint::EndpointId;
use crate::error::{HttpResponseFailure, StatusBody};

use super::response_progress::{ResponseChunkError, ResponseChunkProgress, next_nonempty_chunk};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StatusDisposition {
    RecoverableChallenge,
    Terminal,
}

pub(super) enum StatusCapture {
    SkippedRecoverable,
    Terminal(HttpResponseFailure),
}

#[allow(
    clippy::too_many_arguments,
    reason = "the private status boundary receives one complete set of lifecycle and response-progress owners"
)]
pub(super) async fn collect_terminal_status(
    endpoint: EndpointId,
    method: Method,
    mut response: reqwest::Response,
    disposition: StatusDisposition,
    limit: Option<NonZeroUsize>,
    retry_after_cap: Duration,
    parsed_retry_after: Option<Duration>,
    deadline: Option<tokio::time::Instant>,
    mut progress: ResponseChunkProgress,
) -> StatusCapture {
    if disposition == StatusDisposition::RecoverableChallenge {
        return StatusCapture::SkippedRecoverable;
    }

    let status = response.status();
    let headers = response.headers().clone();
    let retry_after = parsed_retry_after;
    let body = match limit {
        None => StatusBody::disabled(),
        Some(limit)
            if response
                .content_length()
                .is_some_and(|length| length > limit.get() as u64) =>
        {
            StatusBody::omitted_known_too_large()
        }
        Some(limit) => read_complete_body(&mut response, &mut progress, limit, deadline).await,
    };
    StatusCapture::Terminal(HttpResponseFailure::new_with_retry_after(
        endpoint,
        method,
        status,
        headers,
        body,
        retry_after_cap,
        retry_after,
    ))
}

async fn read_complete_body(
    response: &mut reqwest::Response,
    progress: &mut ResponseChunkProgress,
    limit: NonZeroUsize,
    deadline: Option<tokio::time::Instant>,
) -> StatusBody {
    let read = async {
        let mut body = BytesMut::new();
        while let Some(chunk) = next_nonempty_chunk(response, progress).await? {
            let Some(new_len) = body.len().checked_add(chunk.len()) else {
                return Err(ResponseChunkError::LimitExceeded);
            };
            if new_len > limit.get() {
                return Err(ResponseChunkError::LimitExceeded);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body.freeze())
    };

    let result = match deadline {
        Some(deadline) => match tokio::time::timeout_at(deadline, read).await {
            Ok(result) => result,
            Err(_) => return StatusBody::unavailable_deadline(),
        },
        None => read.await,
    };
    match result {
        Ok(bytes) if bytes.is_empty() => StatusBody::empty(),
        Ok(bytes) => StatusBody::complete(bytes),
        Err(ResponseChunkError::LimitExceeded) => StatusBody::omitted_limit_exceeded(),
        Err(ResponseChunkError::Acquisition) => StatusBody::unavailable_read_failure(),
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::io;
    use std::num::NonZeroUsize;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::task::{Context, Poll, Waker};
    use std::time::Duration;

    use bytes::Bytes;
    use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
    use http::{Method, StatusCode};

    use super::*;
    use crate::error::StatusBodyState;
    use crate::http::HttpConfigBuilder;
    use crate::http::build::ManagedHttp;
    use crate::http::response_progress::test_support::{empty_chunks, response_with_status};
    use crate::limits::{MAX_EMPTY_RESPONSE_CHUNKS, MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN};
    use crate::runtime::RuntimeConfigBuilder;

    fn native_status_response(
        chunks: impl IntoIterator<Item = Result<Bytes, io::Error>>,
        polls: &Arc<AtomicUsize>,
        dropped: &Arc<AtomicBool>,
    ) -> reqwest::Response {
        response_with_status(chunks, polls, dropped, StatusCode::BAD_GATEWAY)
    }

    async fn capture_native(response: reqwest::Response, limit: usize) -> HttpResponseFailure {
        match collect_terminal_status(
            EndpointId::from_static("status.capture"),
            Method::GET,
            response,
            StatusDisposition::Terminal,
            Some(NonZeroUsize::new(limit).expect("positive status limit")),
            Duration::from_secs(1),
            None,
            None,
            ResponseChunkProgress::default(),
        )
        .await
        {
            StatusCapture::Terminal(failure) => failure,
            StatusCapture::SkippedRecoverable => {
                panic!("terminal status must capture")
            }
        }
    }

    fn managed_with_limit(limit: Option<NonZeroUsize>) -> ManagedHttp {
        let runtime = match limit {
            Some(limit) => RuntimeConfigBuilder::new().status_body_limit(limit),
            None => RuntimeConfigBuilder::new().disable_status_body_capture(),
        };
        let runtime = runtime.resolve();
        ManagedHttp::build(
            HttpConfigBuilder::new()
                .http1_only()
                .resolve()
                .expect("valid HTTP configuration"),
            &runtime,
        )
        .expect("managed status client should build")
    }

    async fn response(
        managed: &ManagedHttp,
        server: &ScriptedServer,
        path: &str,
    ) -> reqwest::Response {
        managed
            .application
            .get("managed.status", &server.url(path))
            .await
            .expect("terminal response head should arrive")
    }

    async fn terminal(
        managed: &ManagedHttp,
        response: reqwest::Response,
        deadline: Option<tokio::time::Instant>,
    ) -> HttpResponseFailure {
        match managed
            .application
            .terminal_status(
                EndpointId::from_static("managed.status"),
                Method::GET,
                response,
                StatusDisposition::Terminal,
                deadline,
            )
            .await
        {
            StatusCapture::Terminal(failure) => failure,
            StatusCapture::SkippedRecoverable => panic!("terminal response must not be skipped"),
        }
    }

    #[tokio::test]
    async fn complete_empty_and_disabled_states_are_exact() {
        let complete_server =
            ScriptedServer::start([ResponseScript::status(500, b"whole".to_vec())
                .with_header("Retry-After", "120")
                .with_header("Content-Type", "application/test")])
            .expect("complete server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(16).expect("nonzero")));
        let failure = terminal(
            &managed,
            response(&managed, &complete_server, "/complete").await,
            None,
        )
        .await;
        assert_eq!(failure.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(failure.retry_after(), Some(Duration::from_secs(60)));
        assert_eq!(failure.body().state(), StatusBodyState::Complete);
        assert_eq!(
            failure.body().bytes().map(Bytes::as_ref),
            Some(&b"whole"[..])
        );

        let empty_server = ScriptedServer::start([ResponseScript::status(502, Vec::new())])
            .expect("empty server should start");
        let failure = terminal(
            &managed,
            response(&managed, &empty_server, "/empty").await,
            None,
        )
        .await;
        assert_eq!(failure.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(failure.body().state(), StatusBodyState::Empty);
        assert_eq!(failure.body().len(), Some(0));
        assert!(failure.body().bytes().is_none());

        let gate = BodyGate::closed();
        let disabled_server = ScriptedServer::start([ResponseScript::GatedBody {
            status: 503,
            body: vec![b'x'; 64],
            gate: gate.clone(),
        }])
        .expect("disabled server should start");
        let disabled = managed_with_limit(None);
        let failure = tokio::time::timeout(
            Duration::from_millis(300),
            terminal(
                &disabled,
                response(&disabled, &disabled_server, "/disabled").await,
                None,
            ),
        )
        .await
        .expect("disabled capture must not poll the body");
        assert_eq!(failure.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failure.body().state(), StatusBodyState::Disabled);
        assert!(failure.body().bytes().is_none());
        gate.release();
    }

    #[tokio::test]
    async fn known_oversize_is_omitted_without_polling() {
        let gate = BodyGate::closed();
        let server = ScriptedServer::start([ResponseScript::GatedBody {
            status: 413,
            body: vec![b'k'; 64],
            gate: gate.clone(),
        }])
        .expect("known-size server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(8).expect("nonzero")));
        let failure = tokio::time::timeout(
            Duration::from_millis(300),
            terminal(&managed, response(&managed, &server, "/known").await, None),
        )
        .await
        .expect("known oversize must be omitted before polling");
        assert_eq!(failure.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            failure.body().state(),
            StatusBodyState::OmittedKnownTooLarge
        );
        assert!(failure.body().bytes().is_none());
        gate.release();
    }

    #[tokio::test]
    async fn unknown_overflow_discards_every_prefix() {
        let server = ScriptedServer::start([ResponseScript::Chunked {
            status: 500,
            headers: Vec::new(),
            chunks: vec![b"abc".to_vec(), b"def".to_vec()],
        }])
        .expect("chunked server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(4).expect("nonzero")));
        let failure = terminal(
            &managed,
            response(&managed, &server, "/overflow").await,
            None,
        )
        .await;
        assert_eq!(failure.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            failure.body().state(),
            StatusBodyState::OmittedLimitExceeded
        );
        assert_eq!(failure.body().len(), None);
        assert!(failure.body().bytes().is_none());
        assert!(failure.into_body().into_bytes().is_none());
    }

    #[tokio::test]
    async fn read_failure_and_deadline_preserve_the_status() {
        let truncated = ScriptedServer::start([ResponseScript::Truncated {
            status: 502,
            declared_length: 12,
            prefix: b"partial".to_vec(),
        }])
        .expect("truncated server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(32).expect("nonzero")));
        let failure = terminal(
            &managed,
            response(&managed, &truncated, "/truncated").await,
            None,
        )
        .await;
        assert_eq!(failure.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(
            failure.body().state(),
            StatusBodyState::UnavailableReadFailure
        );
        assert!(failure.body().bytes().is_none());

        let gate = BodyGate::closed();
        let stalled = ScriptedServer::start([ResponseScript::GatedChunked {
            status: 504,
            first_chunk: b"first".to_vec(),
            remaining_chunk: b"later".to_vec(),
            gate: gate.clone(),
        }])
        .expect("stalled server should start");
        let deadline = tokio::time::Instant::now() + Duration::from_millis(100);
        let failure = terminal(
            &managed,
            response(&managed, &stalled, "/deadline").await,
            Some(deadline),
        )
        .await;
        assert_eq!(failure.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(failure.body().state(), StatusBodyState::UnavailableDeadline);
        assert!(failure.body().bytes().is_none());
        gate.release();
    }

    #[tokio::test]
    async fn recoverable_challenge_skips_capture_but_terminal_challenge_captures() {
        let gate = BodyGate::closed();
        let recoverable = ScriptedServer::start([ResponseScript::GatedBody {
            status: 401,
            body: vec![b'r'; 64],
            gate: gate.clone(),
        }])
        .expect("recoverable challenge server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(16).expect("nonzero")));
        let capture = tokio::time::timeout(
            Duration::from_millis(300),
            managed.application.terminal_status(
                EndpointId::from_static("managed.challenge"),
                Method::GET,
                response(&managed, &recoverable, "/recoverable").await,
                StatusDisposition::RecoverableChallenge,
                None,
            ),
        )
        .await
        .expect("recoverable challenge must skip body polling");
        assert!(matches!(capture, StatusCapture::SkippedRecoverable));
        gate.release();

        let terminal_server =
            ScriptedServer::start([ResponseScript::status(401, b"challenge".to_vec())])
                .expect("terminal challenge server should start");
        let failure = terminal(
            &managed,
            response(&managed, &terminal_server, "/terminal").await,
            None,
        )
        .await;
        assert_eq!(failure.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(failure.body().state(), StatusBodyState::Complete);
        assert_eq!(
            failure.body().bytes().map(Bytes::as_ref),
            Some(&b"challenge"[..])
        );
    }

    #[tokio::test]
    async fn status_capture_suppresses_empty_chunks() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let failure = capture_native(
            native_status_response(
                [
                    Ok(Bytes::new()),
                    Ok(Bytes::from_static(b"status")),
                    Ok(Bytes::new()),
                ],
                &polls,
                &dropped,
            ),
            64,
        )
        .await;

        assert_eq!(failure.body().state(), StatusBodyState::Complete);
        assert_eq!(
            failure.body().bytes().map(Bytes::as_ref),
            Some(&b"status"[..])
        );
        assert_eq!(polls.load(Ordering::SeqCst), 4);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn status_capture_yields_executor_after_ready_empty_budget() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let response = native_status_response(
            empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN),
            &polls,
            &dropped,
        );
        let mut capture = Box::pin(collect_terminal_status(
            EndpointId::from_static("status.fairness"),
            Method::GET,
            response,
            StatusDisposition::Terminal,
            Some(NonZeroUsize::new(64).expect("positive status limit")),
            Duration::from_secs(1),
            None,
            None,
            ResponseChunkProgress::default(),
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(matches!(capture.as_mut().poll(&mut context), Poll::Pending));
        assert_eq!(
            polls.load(Ordering::SeqCst),
            MAX_EMPTY_RESPONSE_CHUNKS_PER_TURN
        );
    }

    #[tokio::test]
    async fn status_capture_first_excess_is_terminal() {
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let failure = capture_native(
            native_status_response(
                empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1),
                &polls,
                &dropped,
            ),
            64,
        )
        .await;

        assert_eq!(
            failure.body().state(),
            StatusBodyState::OmittedLimitExceeded
        );
        assert!(failure.body().bytes().is_none());
        assert_eq!(polls.load(Ordering::SeqCst), MAX_EMPTY_RESPONSE_CHUNKS + 1);
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn status_capture_remains_bounded_and_redacted() {
        const BODY_SENTINEL: &str = "status-body-secret-8d904";
        const HEADER_SENTINEL: &str = "status-header-secret-172ab";
        let polls = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicBool::new(false));
        let mut chunks = vec![Ok(Bytes::from_static(BODY_SENTINEL.as_bytes()))];
        chunks.extend(empty_chunks(MAX_EMPTY_RESPONSE_CHUNKS + 1));
        let mut response = native_status_response(chunks, &polls, &dropped);
        response.headers_mut().insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static(HEADER_SENTINEL),
        );

        let failure = capture_native(response, 64).await;
        assert_eq!(
            failure.body().state(),
            StatusBodyState::OmittedLimitExceeded
        );
        assert!(failure.body().bytes().is_none());
        let rendered = format!("{failure} {failure:?}");
        assert!(!rendered.contains(BODY_SENTINEL));
        assert!(!rendered.contains(HEADER_SENTINEL));
        assert!(!rendered.contains(&(MAX_EMPTY_RESPONSE_CHUNKS + 1).to_string()));
    }

    #[cfg(feature = "gzip")]
    #[tokio::test]
    async fn limit_accounting_uses_post_decompression_bytes() {
        const GZIP_128_X: &[u8] = &[
            31, 139, 8, 0, 0, 0, 0, 0, 0, 10, 171, 168, 24, 88, 0, 0, 55, 138, 224, 172, 128, 0, 0,
            0,
        ];
        let server = ScriptedServer::start([ResponseScript::Complete {
            status: 500,
            headers: vec![("Content-Encoding".into(), "gzip".into())],
            body: GZIP_128_X.to_vec(),
        }])
        .expect("compressed server should start");
        let managed = managed_with_limit(Some(NonZeroUsize::new(64).expect("nonzero")));
        let response = response(&managed, &server, "/compressed").await;
        assert_eq!(response.content_length(), None);
        let failure = terminal(&managed, response, None).await;
        assert_eq!(
            failure.body().state(),
            StatusBodyState::OmittedLimitExceeded
        );
        assert!(failure.body().bytes().is_none());
    }
}
