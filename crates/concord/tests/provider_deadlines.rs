#![cfg(feature = "default-tls")]

//! Native regressions for the synchronous boundaries of provider acquisition.
//! A callback cannot be preempted; after it returns, an expired or cancelled
//! operation must not hand off a request or install its produced credential.
//! V3R-02/03; acceptance boundaries V3-E04, V3-E14 and V3-E16.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use concord::auth::{
    AccessToken, AuthError, CredentialMaterial, CredentialProvider, CredentialRequest,
    ProviderOrigin, SecretCredential,
};
use concord::error::{AuthErrorKind, DispatchKnowledge, ErrorCategory, ProviderAcquisitionOutcome};
use concord::http::SafeProxy;
use concord::{Binary, Bytes, CancellationToken, api};
use concord_core::contracts::{
    BoxFuture, Method, ProviderBody, ProviderBodyFactory, ProviderRequest, Url,
};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;
use tokio::sync::Notify;

const SHORT: Duration = Duration::from_millis(50);
const SLOW: Duration = Duration::from_millis(200);
const AMPLE: Duration = Duration::from_secs(5);

#[derive(Clone)]
struct Callback {
    first: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    delay: Duration,
    cancel: Option<CancellationToken>,
}

impl Callback {
    fn new(delay: Duration, cancel: Option<CancellationToken>) -> Self {
        Self {
            first: Arc::new(AtomicBool::new(true)),
            calls: Arc::new(AtomicUsize::new(0)),
            delay,
            cancel,
        }
    }

    fn run(&self) {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.first.swap(false, Ordering::SeqCst) {
            if !self.delay.is_zero() {
                std::thread::sleep(self.delay);
            }
            if let Some(token) = &self.cancel {
                token.cancel();
            }
        }
    }
}

impl ProviderBodyFactory for Callback {
    fn create(&self) -> Result<Bytes, AuthError> {
        self.run();
        Ok(Bytes::from_static(b"inert-provider-body"))
    }
}

struct FactoryProvider {
    origins: [ProviderOrigin; 1],
    callback: Callback,
    request_timeout: Duration,
}

impl CredentialProvider for FactoryProvider {
    type Material = AccessToken;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            context
                .http
                .send(
                    ProviderRequest::new(
                        Method::POST,
                        Url::parse("https://localhost/token").unwrap(),
                    )
                    .timeout(self.request_timeout)
                    .body(ProviderBody::Factory(Arc::new(self.callback.clone()))),
                )
                .await?;
            AccessToken::try_new("inert-provider-token").map_err(AuthError::from)
        })
    }

    fn http_origins(&self) -> &[ProviderOrigin] {
        &self.origins
    }
}

api! {
    client FactoryApi {
        base "https://localhost/";
        timeout 5s;
        credentials { credential token = custom FactoryProvider; }
        auth { bearer token; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

fn factory_client(
    server: &TlsServer,
    callback: Callback,
    request_timeout: Duration,
    provider_timeout: Duration,
) -> (FactoryApi, ConnectProxy) {
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let proxy_url = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let client = FactoryApi::builder()
        .token(FactoryProvider {
            origins: [ProviderOrigin::parse("https://localhost").unwrap()],
            callback,
            request_timeout,
        })
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy_url)
                .add_root_certificate_pem(&certificate)
        })
        .configure_runtime(move |runtime| runtime.provider_timeout(provider_timeout))
        .build()
        .unwrap();
    (client, proxy)
}

async fn slow_factory_is_refused(request: Duration, provider: Duration, parent: Duration) {
    let server = TlsServer::start().unwrap();
    server.queue_response(200, b"token-response").unwrap();
    server.queue_response(200, b"application-response").unwrap();
    let callback = Callback::new(SLOW, None);
    let (client, _proxy) = factory_client(&server, callback.clone(), request, provider);
    let error = client
        .protected()
        .timeout(parent)
        .unwrap()
        .await
        .unwrap_err();
    let facts = error.provider_acquisition().unwrap();
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        facts.helper_sends(),
        0,
        "expired factory must not hand off HTTP"
    );
    assert_eq!(facts.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Failed);
    assert_eq!(server.requests().len(), 0);
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    if parent == SHORT {
        assert_eq!(error.category(), ErrorCategory::Timeout);
    } else {
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    }
}

#[tokio::test]
async fn provider_request_timeout_includes_synchronous_body_factory() {
    slow_factory_is_refused(SHORT, AMPLE, AMPLE).await;
}

#[tokio::test]
async fn provider_acquisition_timeout_prevents_handoff_after_body_factory() {
    slow_factory_is_refused(AMPLE, SHORT, AMPLE).await;
}

#[tokio::test]
async fn parent_timeout_prevents_provider_handoff_after_body_factory() {
    slow_factory_is_refused(AMPLE, AMPLE, SHORT).await;
}

#[tokio::test]
async fn cancellation_in_body_factory_prevents_provider_handoff() {
    let server = TlsServer::start().unwrap();
    server.queue_response(200, b"token-response").unwrap();
    let token = CancellationToken::new();
    let callback = Callback::new(Duration::ZERO, Some(token.clone()));
    let (client, _proxy) = factory_client(&server, callback.clone(), AMPLE, AMPLE);
    let error = client
        .protected()
        .cancel_with(token)
        .unwrap()
        .await
        .unwrap_err();
    let facts = error.provider_acquisition().unwrap();
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(facts.outcome(), ProviderAcquisitionOutcome::Cancelled);
    assert_eq!(facts.helper_sends(), 0);
    assert_eq!(facts.dispatch_knowledge(), DispatchKnowledge::NotDispatched);
    assert_eq!(server.requests().len(), 0);
}

#[tokio::test]
async fn body_factory_within_budget_keeps_one_helper_send_and_application_success() {
    let server = TlsServer::start().unwrap();
    server.queue_response(200, b"token-response").unwrap();
    server.queue_response(200, b"application-response").unwrap();
    let callback = Callback::new(Duration::ZERO, None);
    let (client, _proxy) = factory_client(&server, callback.clone(), AMPLE, AMPLE);
    let response = client.protected().response().await.unwrap();
    assert_eq!(
        response.into_value(),
        Bytes::from_static(b"application-response")
    );
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 2);
}

struct ExpirationMaterial(Callback);

impl CredentialMaterial for ExpirationMaterial {
    fn expires_at(&self) -> Option<Instant> {
        self.0.run();
        None
    }
}

impl SecretCredential for ExpirationMaterial {
    fn expose_secret(&self) -> &str {
        "inert-material-token"
    }
}

struct ExpirationProvider {
    callback: Callback,
    acquisitions: Arc<AtomicUsize>,
    gate: Option<Arc<Notify>>,
}

impl CredentialProvider for ExpirationProvider {
    type Material = ExpirationMaterial;

    fn acquire<'a>(
        &'a self,
        _context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>> {
        Box::pin(async move {
            if self.acquisitions.fetch_add(1, Ordering::SeqCst) == 0 {
                if let Some(gate) = &self.gate {
                    gate.notified().await;
                }
            }
            Ok(ExpirationMaterial(self.callback.clone()))
        })
    }
}

api! {
    client ExpirationApi {
        base "https://localhost/";
        timeout 5s;
        credentials { credential token = custom ExpirationProvider; }
        auth { bearer token; }
        GET protected() -> Bytes via Binary { path "protected"; }
    }
}

fn expiration_client(
    server: &TlsServer,
    callback: Callback,
    acquisitions: Arc<AtomicUsize>,
    gate: Option<Arc<Notify>>,
    provider_timeout: Duration,
) -> (ExpirationApi, ConnectProxy) {
    let proxy = ConnectProxy::start(server.address()).unwrap();
    let proxy_url = SafeProxy::parse(proxy.url()).unwrap();
    let certificate = server.certificate_pem().to_owned();
    let client = ExpirationApi::builder()
        .token(ExpirationProvider {
            callback,
            acquisitions,
            gate,
        })
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy_url)
                .add_root_certificate_pem(&certificate)
        })
        .configure_runtime(move |runtime| runtime.provider_timeout(provider_timeout))
        .build()
        .unwrap();
    (client, proxy)
}

async fn slow_expiration_is_not_installed(provider: Duration, parent: Duration) {
    let server = TlsServer::start().unwrap();
    server
        .queue_response(200, b"later-application-response")
        .unwrap();
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let callback = Callback::new(SLOW, None);
    let (client, _proxy) = expiration_client(
        &server,
        callback.clone(),
        Arc::clone(&acquisitions),
        None,
        provider,
    );
    let error = client
        .protected()
        .timeout(parent)
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(callback.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        error.provider_acquisition().unwrap().outcome(),
        ProviderAcquisitionOutcome::Failed
    );
    assert_eq!(server.requests().len(), 0);
    assert_eq!(error.operation_counts().unwrap().application_handoffs(), 0);
    if parent == SHORT {
        assert_eq!(error.category(), ErrorCategory::Timeout);
    } else {
        assert_eq!(error.auth_kind(), Some(AuthErrorKind::Acquisition));
    }
    assert_eq!(
        client.protected().await.unwrap(),
        Bytes::from_static(b"later-application-response")
    );
    assert_eq!(
        acquisitions.load(Ordering::SeqCst),
        2,
        "timed-out material was not cached"
    );
}

#[tokio::test]
async fn provider_timeout_includes_material_expiration_before_installation() {
    slow_expiration_is_not_installed(SHORT, AMPLE).await;
}

#[tokio::test]
async fn parent_timeout_during_expiration_prevents_installation_and_later_reuse() {
    slow_expiration_is_not_installed(AMPLE, SHORT).await;
}

#[tokio::test]
async fn cancellation_in_expiration_prevents_installation_and_later_reuse() {
    let server = TlsServer::start().unwrap();
    server
        .queue_response(200, b"later-application-response")
        .unwrap();
    let token = CancellationToken::new();
    let callback = Callback::new(Duration::ZERO, Some(token.clone()));
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let (client, _proxy) =
        expiration_client(&server, callback, Arc::clone(&acquisitions), None, AMPLE);
    let error = client
        .protected()
        .cancel_with(token)
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(
        error.provider_acquisition().unwrap().outcome(),
        ProviderAcquisitionOutcome::Cancelled
    );
    assert_eq!(server.requests().len(), 0);
    assert!(client.protected().await.is_ok());
    assert_eq!(
        acquisitions.load(Ordering::SeqCst),
        2,
        "cancelled material was not cached"
    );
}

#[tokio::test]
async fn expiration_within_budget_installs_once_and_reuses_material() {
    let server = TlsServer::start().unwrap();
    for _ in 0..2 {
        server.queue_response(200, b"application-response").unwrap();
    }
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let (client, _proxy) = expiration_client(
        &server,
        Callback::new(Duration::ZERO, None),
        Arc::clone(&acquisitions),
        None,
        AMPLE,
    );
    assert!(client.protected().await.is_ok());
    assert!(client.protected().await.is_ok());
    assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 2);
}

#[tokio::test]
async fn material_timeout_wakes_the_shared_cohort_and_allows_later_acquisition() {
    let server = TlsServer::start().unwrap();
    server
        .queue_response(200, b"later-application-response")
        .unwrap();
    let gate = Arc::new(Notify::new());
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let (client, _proxy) = expiration_client(
        &server,
        Callback::new(SLOW, None),
        Arc::clone(&acquisitions),
        Some(gate.clone()),
        SHORT,
    );
    let release = async {
        gate.notify_one();
    };
    // The leader suspends at the gate; the follower joins before release.
    let (leader, follower, ()) = tokio::join!(
        biased;
        async { client.protected().await },
        async { client.protected().await },
        release,
    );
    let leader = leader.unwrap_err();
    let follower = follower.unwrap_err();
    assert_eq!(leader.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(follower.auth_kind(), Some(AuthErrorKind::Acquisition));
    assert_eq!(
        leader.provider_acquisition(),
        follower.provider_acquisition()
    );
    assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(server.requests().len(), 0);
    assert!(client.protected().await.is_ok());
    assert_eq!(acquisitions.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn cancellation_in_expiration_retires_the_leader_and_the_follower_takes_over() {
    let server = TlsServer::start().unwrap();
    server.queue_response(200, b"follower-response").unwrap();
    let gate = Arc::new(Notify::new());
    let token = CancellationToken::new();
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let (client, _proxy) = expiration_client(
        &server,
        Callback::new(Duration::ZERO, Some(token.clone())),
        Arc::clone(&acquisitions),
        Some(Arc::clone(&gate)),
        AMPLE,
    );
    let (leader, follower, ()) = tokio::join!(
        biased;
        async { client.protected().cancel_with(token).unwrap().await },
        async { client.protected().await },
        async { gate.notify_one(); },
    );
    let leader = leader.unwrap_err();
    assert_eq!(leader.category(), ErrorCategory::Cancelled);
    assert_eq!(
        leader.provider_acquisition().unwrap().outcome(),
        ProviderAcquisitionOutcome::Cancelled
    );
    assert_eq!(follower.unwrap(), Bytes::from_static(b"follower-response"));
    assert_eq!(acquisitions.load(Ordering::SeqCst), 2);
    assert_eq!(server.requests().len(), 1);
}
