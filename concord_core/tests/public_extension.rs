#![cfg_attr(not(feature = "dangerous-dev-tools"), allow(dead_code, unused_imports))]

use bytes::Bytes;
use concord_core::advanced::{
    AdvancedRequestBody, AuthChallengeMode, AuthError, AuthFuture, AuthPreparationMode,
    AuthProviderBinding, CredentialContext, CredentialId, CredentialProvider,
    CredentialProviderState, InvalidateReason, OctetStream, PreparedBody, PreparedEndpoint,
    PreparedRequestEntity, PreparedStreamEndpoint, RequestAuthentication, RequestEntity,
};
use concord_core::prelude::{
    ApiClient, ApiClientError, ApiKey, ClientContext, RequestExecutionMeta, RetryMode, Text,
};
use http::{Method, StatusCode};
use http_body::{Body, Frame, SizeHint};
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

#[cfg(feature = "dangerous-dev-tools")]
#[path = "../../concord_test_support/src/deterministic.rs"]
mod deterministic_mock;
#[cfg(feature = "dangerous-dev-tools")]
use deterministic_mock::{ScriptedReply, deterministic_mock};

struct LocalRequestEntity;

impl RequestEntity for LocalRequestEntity {
    type Input = PreparedBody;

    fn prepare(
        body: Self::Input,
        _ctx: concord_core::advanced::ErrorContext,
    ) -> Result<PreparedRequestEntity, ApiClientError> {
        Ok(PreparedRequestEntity { body })
    }
}

struct LocalBody {
    bytes: Option<Bytes>,
}

impl LocalBody {
    fn new(bytes: &'static [u8]) -> Self {
        Self {
            bytes: Some(Bytes::from_static(bytes)),
        }
    }
}

impl Body for LocalBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        Poll::Ready(self.bytes.take().map(Frame::data).map(Ok))
    }

    fn is_end_stream(&self) -> bool {
        self.bytes.is_none()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact(self.bytes.as_ref().map_or(0, |bytes| bytes.len()) as u64)
    }
}

#[derive(Clone)]
struct PublicContext;

#[derive(Clone)]
struct PublicAuthVars {
    acquired: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

#[derive(Clone)]
struct PublicAuthState {
    provider: Arc<CredentialProviderState<PublicContext, PublicProvider>>,
}

fn empty_auth_vars() -> PublicAuthVars {
    PublicAuthVars {
        acquired: Arc::new(AtomicUsize::new(0)),
        invalidated: Arc::new(AtomicUsize::new(0)),
    }
}

#[test]
fn downstream_generic_client_has_only_supported_retry_modes() {
    ApiClient::<PublicContext>::with_retry_mode((), empty_auth_vars(), RetryMode::ProtocolRecovery)
        .expect("protocol recovery remains available");
    ApiClient::<PublicContext>::with_retry_mode((), empty_auth_vars(), RetryMode::Disabled)
        .expect("disabled mode remains available");
    let status = RetryMode::status(1, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();
    let error = match ApiClient::<PublicContext>::with_retry_mode((), empty_auth_vars(), status) {
        Ok(_) => panic!("generic clients have no status authority"),
        Err(error) => error,
    };
    assert!(matches!(
        error,
        concord_core::prelude::RetryModeError::NotFixedOrigin
    ));
}

impl ClientContext for PublicContext {
    type Vars = ();
    type AuthVars = PublicAuthVars;
    type AuthState = PublicAuthState;

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.com";
    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        PublicAuthState {
            provider: Arc::new(CredentialProviderState::new(PublicProvider {
                acquired: auth.acquired.clone(),
                invalidated: auth.invalidated.clone(),
            })),
        }
    }

    fn auth_provider_binding<'a>(
        credential: &CredentialId,
        state: &'a Self::AuthState,
    ) -> Option<AuthProviderBinding<'a, Self>> {
        (credential == &CredentialId::new("public", "token")).then(|| {
            state.provider.secret_binding(
                AuthPreparationMode::PerExecution,
                AuthChallengeMode::Refresh,
            )
        })
    }
}

#[derive(Clone)]
struct PublicProvider {
    acquired: Arc<AtomicUsize>,
    invalidated: Arc<AtomicUsize>,
}

impl CredentialProvider<PublicContext> for PublicProvider {
    type Credential = ApiKey;

    fn id(&self) -> CredentialId {
        CredentialId::new("public", "token")
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        Box::pin(async move {
            let generation = self.acquired.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(ApiKey::new(format!("fixture-token-{generation}")))
        })
    }

    fn invalidate<'a>(
        &'a self,
        _ctx: CredentialContext<'a, PublicContext>,
        _current: Option<&'a Self::Credential>,
        _reason: InvalidateReason,
    ) -> AuthFuture<'a, Result<(), AuthError>> {
        Box::pin(async move {
            self.invalidated.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    }
}

fn advanced_body(bytes: &'static [u8]) -> AdvancedRequestBody {
    AdvancedRequestBody::new(LocalBody::new(bytes))
}

fn endpoint(
    name: &'static str,
    path: &'static str,
    body: PreparedBody,
) -> PreparedEndpoint<Text<String>> {
    PreparedEndpoint::from_request_entity::<LocalRequestEntity>(name, Method::POST, path, body)
        .expect("public request entity")
}

#[cfg(feature = "dangerous-dev-tools")]
#[tokio::test]
async fn downstream_public_extension_executes_without_generated_integration_modules() {
    let (mock, handle) = deterministic_mock()
        .replies([
            ScriptedReply::ok_text(Bytes::from_static(b"one-shot"))
                .expect_body(Bytes::from_static(b"one-shot-body")),
            ScriptedReply::ok_text(Bytes::from_static(b"factory"))
                .expect_body(Bytes::from_static(b"factory-body")),
            ScriptedReply::status(StatusCode::UNAUTHORIZED)
                .expect_body(Bytes::from_static(b"recovery-body"))
                .expect_header(http::header::AUTHORIZATION, "Bearer fixture-token-1"),
            ScriptedReply::ok_text(Bytes::from_static(b"authenticated"))
                .expect_body(Bytes::from_static(b"recovery-body"))
                .expect_header(http::header::AUTHORIZATION, "Bearer fixture-token-2"),
            ScriptedReply::ok_text(Bytes::from_static(b"metadata")),
            ScriptedReply::status(StatusCode::OK)
                .with_header(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/octet-stream"),
                )
                .with_body(Bytes::from_static(b"stream")),
        ])
        .build();
    let acquired = Arc::new(AtomicUsize::new(0));
    let invalidated = Arc::new(AtomicUsize::new(0));
    let client = ApiClient::<PublicContext>::with_safe_reqwest_builder_and_retry_mode(
        (),
        PublicAuthVars {
            acquired: acquired.clone(),
            invalidated: invalidated.clone(),
        },
        RetryMode::ProtocolRecovery,
        |builder| Ok(mock.configure_application(builder)),
    )
    .expect("fixed-origin managed client");

    let one_shot = PreparedBody::one_shot(advanced_body(b"one-shot-body"), None);
    assert_eq!(
        endpoint("OneShot", "/one-shot", one_shot)
            .execute(&client)
            .await
            .expect("one-shot endpoint"),
        "one-shot"
    );

    let factory_calls = Arc::new(AtomicUsize::new(0));
    let observed = factory_calls.clone();
    let factory = PreparedBody::factory(SizeHint::with_exact(12), None, move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(advanced_body(b"factory-body"))
    });
    assert_eq!(
        endpoint("Factory", "/factory", factory)
            .execute(&client)
            .await
            .expect("factory endpoint"),
        "factory"
    );
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);

    let recovery_calls = Arc::new(AtomicUsize::new(0));
    let observed = recovery_calls.clone();
    let recovery_body = PreparedBody::factory(SizeHint::with_exact(13), None, move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Ok(advanced_body(b"recovery-body"))
    });
    let authenticated = endpoint("Authenticated", "/authenticated", recovery_body)
        .authentication(RequestAuthentication::bearer(CredentialId::new(
            "public", "token",
        )))
        .execute(&client)
        .await
        .expect("one bounded authentication recovery");
    assert_eq!(authenticated, "authenticated");
    assert_eq!(recovery_calls.load(Ordering::SeqCst), 2);
    assert_eq!(acquired.load(Ordering::SeqCst), 2);
    assert_eq!(invalidated.load(Ordering::SeqCst), 1);

    let decoded = endpoint(
        "BufferedMetadata",
        "/buffered-metadata",
        PreparedBody::empty(),
    )
    .response(&client)
    .await
    .expect("buffered metadata");
    let buffered_meta: &RequestExecutionMeta = decoded.meta();
    assert_eq!(buffered_meta.endpoint, "BufferedMetadata");

    let mut streamed = PreparedStreamEndpoint::<OctetStream>::new(
        "StreamMetadata",
        Method::GET,
        "/stream-metadata",
        PreparedRequestEntity {
            body: PreparedBody::empty(),
        },
    )
    .execute(&client)
    .await
    .expect("stream metadata");
    let stream_meta: &RequestExecutionMeta = streamed.meta();
    assert_eq!(stream_meta.endpoint, "StreamMetadata");
    assert_eq!(
        streamed.next_chunk().await.expect("stream chunk"),
        Some(Bytes::from_static(b"stream"))
    );

    let requests = handle.recorded();
    assert_eq!(requests.len(), 6);
    assert_eq!(requests[0].known_body_length, Some(13));
    assert_eq!(requests[1].known_body_length, Some(12));
    assert!(
        requests[2]
            .protected_header_names
            .contains(&http::header::AUTHORIZATION)
    );
    assert!(
        requests[3]
            .protected_header_names
            .contains(&http::header::AUTHORIZATION)
    );
    handle.finish();
}
