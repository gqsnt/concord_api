use std::num::NonZeroUsize;
use std::time::Duration;

use bytes::BytesMut;
use http::Method;

use crate::endpoint::EndpointId;
use crate::error::{HttpResponseFailure, StatusBody};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum StatusDisposition {
    RecoverableChallenge,
    Terminal,
}

pub(super) enum StatusCapture {
    SkippedRecoverable,
    Terminal(HttpResponseFailure),
}

pub(super) async fn collect_terminal_status(
    endpoint: EndpointId,
    method: Method,
    mut response: reqwest::Response,
    disposition: StatusDisposition,
    limit: Option<NonZeroUsize>,
    retry_after_cap: Duration,
    deadline: Option<tokio::time::Instant>,
) -> StatusCapture {
    if disposition == StatusDisposition::RecoverableChallenge {
        return StatusCapture::SkippedRecoverable;
    }

    let status = response.status();
    let headers = response.headers().clone();
    let body = match limit {
        None => StatusBody::disabled(),
        Some(limit)
            if response
                .content_length()
                .is_some_and(|length| length > limit.get() as u64) =>
        {
            StatusBody::omitted_known_too_large()
        }
        Some(limit) => read_complete_body(&mut response, limit, deadline).await,
    };
    StatusCapture::Terminal(HttpResponseFailure::new(
        endpoint,
        method,
        status,
        headers,
        body,
        retry_after_cap,
    ))
}

async fn read_complete_body(
    response: &mut reqwest::Response,
    limit: NonZeroUsize,
    deadline: Option<tokio::time::Instant>,
) -> StatusBody {
    let read = async {
        let mut body = BytesMut::new();
        while let Some(chunk) = response.chunk().await? {
            let Some(new_len) = body.len().checked_add(chunk.len()) else {
                return Ok::<_, reqwest::Error>(Err(()));
            };
            if new_len > limit.get() {
                return Ok(Err(()));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(Ok(body.freeze()))
    };

    let result = match deadline {
        Some(deadline) => match tokio::time::timeout_at(deadline, read).await {
            Ok(result) => result,
            Err(_) => return StatusBody::unavailable_deadline(),
        },
        None => read.await,
    };
    match result {
        Ok(Ok(bytes)) if bytes.is_empty() => StatusBody::empty(),
        Ok(Ok(bytes)) => StatusBody::complete(bytes),
        Ok(Err(())) => StatusBody::omitted_limit_exceeded(),
        Err(_) => StatusBody::unavailable_read_failure(),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use bytes::Bytes;
    use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
    use http::{Method, StatusCode};

    use super::*;
    use crate::error::StatusBodyState;
    use crate::http::HttpConfigBuilder;
    use crate::http::build::ManagedHttp;
    use crate::runtime::RuntimeConfigBuilder;

    fn managed_with_limit(limit: Option<NonZeroUsize>) -> ManagedHttp {
        let runtime = match limit {
            Some(limit) => RuntimeConfigBuilder::new().status_body_limit(limit),
            None => RuntimeConfigBuilder::new().disable_status_body_capture(),
        };
        let runtime = runtime.resolve();
        ManagedHttp::build(HttpConfigBuilder::new().http1_only().resolve(), &runtime)
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
