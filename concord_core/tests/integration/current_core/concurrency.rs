use super::common::*;
use concord_core::advanced::{
    AuthAppliedCredential, AuthDecision, AuthError, AuthFuture, AuthPlacement, AuthRequirement,
    AuthStepPolicy, BuiltRequest, BuiltResponse, CacheAfter, CacheBefore, CacheFuture,
    CacheRevalidation, CacheStore, CredentialContext, CredentialId, CredentialProvider,
    CredentialRefreshReason, CredentialSlot, PreparedAuthCredential, RequestMeta,
    apply_secret_credential,
};
use concord_core::prelude::{AccessToken, ApiClient, ApiClientError, ClientContext, Endpoint};
use http::{HeaderMap, HeaderValue, StatusCode};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Notify, watch};

#[tokio::test]
async fn identical_concurrent_get_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));

    let a = spawn_text_request(client.clone(), TextEndpoint::default());
    let b = spawn_text_request(client, TextEndpoint::default());

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn identical_concurrent_post_requests_are_not_coalesced() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));
    let endpoint = TextEndpoint {
        method: http::Method::POST,
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn cache_hit_after_completed_response_still_avoids_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = MockTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "stored"),
            MockResponse::text(StatusCode::OK, "unexpected"),
        ],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let first = client.request(endpoint.clone()).execute_decoded().await?;
    let second = client.request(endpoint).execute_decoded().await?;

    assert_eq!(first.value(), "stored");
    assert_eq!(second.value(), "stored");
    assert_eq!(sent.sent_count().await, 1);
    Ok(())
}

#[tokio::test]
async fn concurrent_cache_miss_requests_both_send_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(StoringCache::default());
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), None);
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 2);
    Ok(())
}

#[tokio::test]
async fn concurrent_fresh_cache_hits_bypass_transport() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let cache = Arc::new(RecordingCache::hit(
        events.clone(),
        built_response("Text", StatusCode::OK, "cached"),
    ));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = MockTransport::new(
        events.clone(),
        vec![MockResponse::text(StatusCode::OK, "unexpected")],
    );
    let sent = transport.clone();
    let mut client = client(TestAuthVars::default(), transport);
    configure_runtime(&mut client, Some(cache), Some(limiter));
    let client = Arc::new(client);
    let endpoint = TextEndpoint {
        policy: cache_policy(),
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    let values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    assert_eq!(values, vec!["cached".to_string(), "cached".to_string()]);
    assert_eq!(sent.sent_count().await, 0);

    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        0
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "transport")
            .count(),
        0
    );
    Ok(())
}

#[tokio::test]
async fn rate_limit_still_observes_each_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let limiter = Arc::new(RecordingRateLimiter::new(events.clone()));
    let transport = GateTransport::new(
        events.clone(),
        vec![
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let mut client = ApiClient::<TestCx, _>::with_transport((), TestAuthVars::default(), transport);
    configure_runtime(&mut client, None, Some(limiter));
    let client = Arc::new(client);

    let a = spawn_text_request(client.clone(), TextEndpoint::default());
    let b = spawn_text_request(client, TextEndpoint::default());

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    a.await.expect("request task panicked")?;
    b.await.expect("request task panicked")?;

    let events = events.lock().await.clone();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_acquire")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.as_str() == "rate_response")
            .count(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn retry_still_applies_per_non_coalesced_request() -> Result<(), ApiClientError> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let transport = GateTransport::new(
        events,
        vec![
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-a"),
            MockResponse::text(StatusCode::INTERNAL_SERVER_ERROR, "retry-b"),
            MockResponse::text(StatusCode::OK, "first"),
            MockResponse::text(StatusCode::OK, "second"),
        ],
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<TestCx, _>::with_transport(
        (),
        TestAuthVars::default(),
        transport,
    ));
    let endpoint = TextEndpoint {
        policy: retry_policy(2),
        ..Default::default()
    };

    let a = spawn_text_request(client.clone(), endpoint.clone());
    let b = spawn_text_request(client, endpoint);

    sent.wait_for_sends(2).await;
    assert_eq!(sent.sent_count().await, 2);
    sent.release_all();

    let mut values = vec![
        a.await.expect("request task panicked")?,
        b.await.expect("request task panicked")?,
    ];
    values.sort();
    assert_eq!(values, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(sent.sent_count().await, 4);
    Ok(())
}

#[tokio::test]
async fn concurrent_missing_credential_acquisition_single_flights() -> Result<(), ApiClientError> {
    const N: usize = 4;

    let events = Arc::new(Mutex::new(Vec::new()));
    let provider = ControlledTokenProvider::new("shared-token");
    let transport = GateTransport::new(
        events,
        (0..N)
            .map(|index| MockResponse::text(StatusCode::OK, format!("ok-{index}")))
            .collect(),
    );
    let sent = transport.clone();
    let client = Arc::new(ApiClient::<SingleFlightCx, _>::with_transport(
        (),
        SingleFlightAuthVars {
            provider: provider.clone(),
        },
        transport,
    ));
    let endpoint = TextEndpoint {
        policy: auth_policy(AuthPlacement::Bearer),
        ..Default::default()
    };

    let tasks = (0..N)
        .map(|_| spawn_single_flight_request(client.clone(), endpoint.clone()))
        .collect::<Vec<_>>();

    provider.wait_for_acquires(1).await;
    assert_eq!(provider.acquire_count().await, 1);
    assert_eq!(sent.sent_count().await, 0);

    provider.release_all();
    sent.wait_for_sends(N).await;
    assert_eq!(provider.acquire_count().await, 1);
    assert_eq!(sent.sent_count().await, N);

    let requests = sent.requests().await;
    assert_eq!(requests.len(), N);
    for request in &requests {
        assert_eq!(
            request.headers.get(http::header::AUTHORIZATION),
            Some(&HeaderValue::from_static("Bearer shared-token"))
        );
    }

    sent.release_all();

    let mut values = Vec::new();
    for task in tasks {
        values.push(task.await.expect("request task panicked")?);
    }
    values.sort();
    assert_eq!(
        values,
        (0..N)
            .map(|index| format!("ok-{index}"))
            .collect::<Vec<_>>()
    );
    Ok(())
}

fn spawn_text_request<T>(
    client: Arc<ApiClient<TestCx, T>>,
    endpoint: TextEndpoint,
) -> tokio::task::JoinHandle<Result<String, ApiClientError>>
where
    T: concord_core::advanced::Transport + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        client
            .request(endpoint)
            .execute_decoded()
            .await
            .map(|response| response.into_value())
    })
}

fn spawn_single_flight_request<T>(
    client: Arc<ApiClient<SingleFlightCx, T>>,
    endpoint: TextEndpoint,
) -> tokio::task::JoinHandle<Result<String, ApiClientError>>
where
    T: concord_core::advanced::Transport + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        client
            .request(endpoint)
            .execute_decoded()
            .await
            .map(|response| response.into_value())
    })
}

#[derive(Default)]
struct StoringCache {
    response: Mutex<Option<BuiltResponse>>,
}

impl CacheStore for StoringCache {
    fn before_request<'a>(&'a self, _request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            match self.response.lock().await.clone() {
                Some(response) => CacheBefore::Hit(response),
                None => CacheBefore::Miss,
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            *self.response.lock().await = Some(response.clone());
            CacheAfter::Stored
        })
    }
}

#[derive(Clone)]
struct SingleFlightCx;

#[derive(Clone)]
struct SingleFlightAuthVars {
    provider: ControlledTokenProvider,
}

#[derive(Clone)]
struct SingleFlightAuthState {
    token: Arc<CredentialSlot<SingleFlightCx, ControlledTokenProvider>>,
}

impl ClientContext for SingleFlightCx {
    type Vars = ();
    type AuthVars = SingleFlightAuthVars;
    type AuthState = SingleFlightAuthState;
    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_vars: &Self::Vars, auth: &Self::AuthVars) -> Self::AuthState {
        SingleFlightAuthState {
            token: Arc::new(CredentialSlot::new(auth.provider.clone())),
        }
    }

    fn prepare_auth_requirement<'a>(
        requirement: &'a AuthRequirement,
        request: &'a mut concord_core::advanced::AuthApplicationRequest<'_>,
        vars: &'a Self::Vars,
        auth: &'a Self::AuthVars,
        auth_state: &'a Self::AuthState,
        executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
    ) -> AuthFuture<'a, Result<PreparedAuthCredential, AuthError>> {
        Box::pin(async move {
            let ctx = CredentialContext {
                vars,
                auth,
                auth_state,
                executor,
                credential_id: requirement.credential.id.clone(),
                reason: CredentialRefreshReason::Missing,
            };
            let lease = auth_state
                .token
                .get_or_refresh(ctx, AuthStepPolicy::default())
                .await?;
            let application = apply_secret_credential(request, requirement, &lease.value)?;
            let applied = AuthAppliedCredential {
                credential_id: requirement.credential.id.clone(),
                usage_id: requirement.usage_id.clone(),
                step_id: requirement.step_id,
                generation: Some(lease.generation),
                identity: application.identity().clone(),
                provenance: requirement.provenance.clone(),
            };
            Ok(PreparedAuthCredential::new(applied, application))
        })
    }

    fn handle_auth_response<'a>(
        _requirement: &'a AuthRequirement,
        _applied: &'a AuthAppliedCredential,
        _vars: &'a Self::Vars,
        _auth: &'a Self::AuthVars,
        _auth_state: &'a Self::AuthState,
        _executor: &'a dyn concord_core::advanced::AuthHttpExecutor,
        _meta: &'a RequestMeta,
        _status: StatusCode,
        _headers: &'a HeaderMap,
    ) -> AuthFuture<'a, Result<AuthDecision, AuthError>> {
        Box::pin(async { Ok(AuthDecision::Continue) })
    }
}

impl Endpoint<SingleFlightCx> for TextEndpoint {
    type Response = String;

    fn plan(
        &self,
        _ctx: &concord_core::internal::ClientPlanContext<'_, SingleFlightCx>,
    ) -> Result<concord_core::internal::RequestPlan, ApiClientError> {
        Ok(request_plan(
            self.name,
            self.method.clone(),
            self.path,
            self.policy.clone(),
            self.pagination.clone(),
            decode_string,
        ))
    }
}

#[derive(Clone)]
struct ControlledTokenProvider {
    id: CredentialId,
    token: &'static str,
    acquire_count: Arc<Mutex<usize>>,
    acquired: Arc<Notify>,
    release: watch::Sender<bool>,
}

impl ControlledTokenProvider {
    fn new(token: &'static str) -> Self {
        let (release, _) = watch::channel(false);
        Self {
            id: CredentialId::new("test", "token"),
            token,
            acquire_count: Arc::new(Mutex::new(0)),
            acquired: Arc::new(Notify::new()),
            release,
        }
    }

    async fn acquire_count(&self) -> usize {
        *self.acquire_count.lock().await
    }

    async fn wait_for_acquires(&self, expected: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let notified = self.acquired.notified();
                if self.acquire_count().await >= expected {
                    break;
                }
                notified.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {expected} credential acquisitions"));
    }

    fn release_all(&self) {
        let _ = self.release.send(true);
    }
}

impl CredentialProvider<SingleFlightCx> for ControlledTokenProvider {
    type Credential = AccessToken;

    fn id(&self) -> CredentialId {
        self.id.clone()
    }

    fn acquire<'a>(
        &'a self,
        _ctx: CredentialContext<'a, SingleFlightCx>,
    ) -> AuthFuture<'a, Result<Self::Credential, AuthError>> {
        let mut release = self.release.subscribe();
        Box::pin(async move {
            *self.acquire_count.lock().await += 1;
            self.acquired.notify_waiters();

            while !*release.borrow() {
                if release.changed().await.is_err() {
                    break;
                }
            }

            Ok(AccessToken::new(self.token.to_string()))
        })
    }
}
