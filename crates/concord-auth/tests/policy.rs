//! Executable SDK recipes: SendWith's single challenge replay and a single concord-retry loop.
#![allow(missing_docs)]

use std::{
    convert::Infallible,
    error::Error,
    num::NonZeroUsize,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use concord::{Endpoint, Received, SendWith};
use concord_auth::{Snapshot, Token, TokenCache, bearer};
use concord_retry::{Decision, Policy, Replay, Retry, StopReason, concord::ReplayMode};
use concord_testserver::{Reply, TestServer};
use futures_util::{FutureExt, future::BoxFuture};
use reqwest::{StatusCode, header::AUTHORIZATION};

type BoxError = Box<dyn Error + Send + Sync>;
type Acquire = Arc<dyn Fn() -> BoxFuture<'static, Result<Token, BoxError>> + Send + Sync>;

// An SDK chooses these fields, including its destination, tenant, principal and access contract.
#[derive(Clone, Eq, PartialEq, Hash)]
struct Scope {
    audience: String,
    tenant: &'static str,
    principal: &'static str,
    access: &'static str,
}

#[derive(Clone)]
pub struct AuthPolicy {
    cache: Arc<TokenCache<Scope, BoxError>>,
    scope: Scope,
    acquire: Acquire,
    replay: bool, // semantic permission from the SDK's operation contract, independent of try_clone
    application_attempts: Arc<AtomicUsize>,
}

impl AuthPolicy {
    async fn current(&self) -> Result<Snapshot, BoxError> {
        let acquire = self.acquire.clone();
        Ok(self
            .cache
            .get_or_refresh(self.scope.clone(), move || acquire())
            .await?)
    }
}

impl<T: Send> SendWith<T> for AuthPolicy {
    type Output = Result<Received<T>, BoxError>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let (client, mut request, decoder) = endpoint.prepare()?.into_parts();
        // The SDK binds this policy to one destination. Never apply its credential elsewhere.
        assert_eq!(
            request.url().origin().ascii_serialization(),
            self.scope.audience
        );
        let template = self.replay.then(|| request.try_clone()).flatten();
        let used = self.current().await?;
        request
            .headers_mut()
            .insert(AUTHORIZATION, used.header().clone());
        self.application_attempts.fetch_add(1, Ordering::SeqCst);
        let received = decoder.execute(&client, request).await?;
        if received.status() != StatusCode::UNAUTHORIZED {
            return Ok(received);
        }
        self.cache.invalidate(&self.scope, &used.generation());
        let Some(mut request) = template else {
            return Ok(received);
        };
        drop(received); // the SDK may instead read a bounded error document
        let used = self.current().await?;
        request
            .headers_mut()
            .insert(AUTHORIZATION, used.header().clone());
        self.application_attempts.fetch_add(1, Ordering::SeqCst);
        let received = decoder.execute(&client, request).await?;
        if received.status() == StatusCode::UNAUTHORIZED {
            self.cache.invalidate(&self.scope, &used.generation());
        }
        Ok(received) // no further refresh or replay; SDK interprets this head/body once
    }
}

struct NativeBody;
impl concord::Encode<reqwest::Body> for NativeBody {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(body: reqwest::Body) -> Result<reqwest::Body, concord::CodecError> {
        Ok(body)
    }
}

concord::api! {
    pub client Api(auth: AuthPolicy, header("idempotency-key") idempotency: String) {
        base "http://unused.invalid";
        send auth;
        GET get() { path "resource"; }
        POST put(body value: reqwest::Body) { request NativeBody; path "resource"; }
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .build()
        .unwrap()
}

fn policy(server: &TestServer, tokens: &TestServer, replay: bool) -> AuthPolicy {
    let client = http();
    let url = tokens.url();
    AuthPolicy {
        cache: Arc::new(TokenCache::new(
            NonZeroUsize::new(4).unwrap(),
            Duration::ZERO,
        )),
        scope: Scope {
            audience: server.url(),
            tenant: "tenant-a",
            principal: "user-a",
            access: "read-write",
        },
        acquire: Arc::new(move || {
            let client = client.clone();
            let url = url.clone();
            async move {
                // The SDK owns the token service protocol. This fixture returns a plain token.
                let text = client
                    .get(url)
                    .send()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                Ok(Token::new(bearer(&text)?, Duration::from_secs(60)))
            }
            .boxed()
        }),
        replay,
        application_attempts: Arc::new(AtomicUsize::new(0)),
    }
}

fn api(server: &TestServer, policy: AuthPolicy) -> Api {
    Api::new(
        http(),
        ApiConfig {
            auth: policy,
            idempotency: "stable-operation-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap()
}

async fn tokens() -> TestServer {
    let count = AtomicUsize::new(0);
    TestServer::start(move |_| {
        Reply::new(200).body(format!(
            "token-{}",
            count.fetch_add(1, Ordering::SeqCst) + 1
        ))
    })
    .await
}

#[tokio::test]
async fn ordinary_send_uses_current_token_and_replays_one_401_with_a_new_generation() {
    let server = TestServer::start(|r| {
        Reply::new(if r.header("authorization") == Some("Bearer token-1") {
            401
        } else {
            200
        })
    })
    .await;
    let tokens = tokens().await;
    let policy = policy(&server, &tokens, true);
    let api = api(&server, policy.clone());
    let received = api
        .put(reqwest::Body::from("same body"))
        .send()
        .await
        .unwrap();
    assert_eq!(received.status(), StatusCode::OK);
    assert_eq!(received.operation().unwrap().name(), "Api.put");
    received.decode().await.unwrap();
    api.get().send().await.unwrap().decode().await.unwrap();
    assert_eq!(
        tokens.requests().len(),
        2,
        "token service requests are separate"
    );
    assert_eq!(policy.application_attempts.load(Ordering::SeqCst), 3);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].body, requests[1].body);
    assert_eq!(
        requests[0].header("idempotency-key"),
        requests[1].header("idempotency-key")
    );
    assert_eq!(requests[0].header("authorization"), Some("Bearer token-1"));
    assert_eq!(requests[1].header("authorization"), Some("Bearer token-2"));
    assert_eq!(requests[2].header("authorization"), Some("Bearer token-2"));
    server.shutdown().await;
    tokens.shutdown().await;
}

#[tokio::test]
async fn a_second_401_is_returned_without_another_acquisition_or_replay() {
    let server = TestServer::start(|_| Reply::new(401)).await;
    let tokens = tokens().await;
    let policy = policy(&server, &tokens, true);
    let received = api(&server, policy.clone()).get().send().await.unwrap();
    assert_eq!(received.status(), StatusCode::UNAUTHORIZED);
    assert!(policy.cache.get(&policy.scope).is_none());
    assert_eq!(tokens.requests().len(), 2);
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
    tokens.shutdown().await;
}

#[tokio::test]
async fn refresh_permission_does_not_grant_replay_permission() {
    let server = TestServer::start(|_| Reply::new(401)).await;
    let tokens = tokens().await;
    let policy = policy(&server, &tokens, false);
    let received = api(&server, policy)
        .put(reqwest::Body::from("cloneable but not permitted"))
        .send()
        .await
        .unwrap();
    assert_eq!(received.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(tokens.requests().len(), 1);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    tokens.shutdown().await;
}

#[tokio::test]
async fn one_shot_body_is_sent_once_without_buffering_for_replay() {
    let server = TestServer::start(|_| Reply::new(401)).await;
    let tokens = tokens().await;
    let policy = policy(&server, &tokens, true);
    let body = reqwest::Body::wrap_stream(futures_util::stream::once(async {
        Ok::<_, Infallible>("streamed once")
    }));
    let received = api(&server, policy).put(body).send().await.unwrap();
    assert_eq!(received.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(tokens.requests().len(), 1);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.requests()[0].body_text(), "streamed once");
    server.shutdown().await;
    tokens.shutdown().await;
}

#[tokio::test]
async fn async_acquisition_in_retry_attempt_hook_runs_after_wait_with_no_nested_send_policy() {
    let server = TestServer::start(|r| {
        Reply::new(if r.header("authorization") == Some("Bearer token-1") {
            401
        } else {
            200
        })
    })
    .await;
    let tokens = tokens().await;
    let mut auth = policy(&server, &tokens, true);
    let epoch = Instant::now();
    let clock = Arc::new(Mutex::new(Duration::ZERO));
    let token_clock = clock.clone();
    auth.cache = Arc::new(TokenCache::with_clock(
        NonZeroUsize::new(4).unwrap(),
        Duration::ZERO,
        move || epoch + *token_clock.lock().unwrap(),
    ));
    // Prepare directly: calling generated send() here would compose two replay loops.
    let (client, request, decoder) = api(&server, auth.clone())
        .put(reqwest::Body::from("stable bytes"))
        .prepare()
        .unwrap()
        .into_parts();
    let replay_available = request.try_clone().is_some();
    let permission = if auth.replay && replay_available {
        Replay::Permitted
    } else {
        Replay::Never
    };
    let mut template = Some(request);
    let mut p = Policy::new(NonZeroUsize::new(3).unwrap());
    p.initial_delay = Duration::from_secs(1);
    p.max_delay = Duration::from_secs(1);
    let used = Arc::new(Mutex::new(None));
    let hook_used = used.clone();
    let hook_auth = auth.clone();
    let hook_clock = clock.clone();
    let run = Retry::new(p, permission)
        .run(
            |attempt| {
                let mut request = template
                    .as_ref()
                    .and_then(reqwest::Request::try_clone)
                    .or_else(|| template.take())
                    .unwrap();
                let auth = hook_auth.clone();
                let used = hook_used.clone();
                let clock = hook_clock.clone();
                let client = client.clone();
                async move {
                    // Async attempt hook: current() may acquire; exactly one application send.
                    assert_eq!(
                        *clock.lock().unwrap(),
                        Duration::from_secs((attempt - 1) as u64)
                    );
                    let token = auth.current().await?;
                    *used.lock().unwrap() = Some(token.generation());
                    request
                        .headers_mut()
                        .insert(AUTHORIZATION, token.header().clone());
                    auth.application_attempts.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, BoxError>(decoder.execute(&client, request).await?)
                }
            },
            |outcome| match outcome {
                Ok(received) if received.status() == StatusCode::UNAUTHORIZED => {
                    if let Some(generation) = used.lock().unwrap().as_ref() {
                        auth.cache.invalidate(&auth.scope, generation);
                    }
                    Decision::RetryAfter(None) // permission/count/deadline still belong to Retry
                }
                _ => Decision::Done,
            },
            || *clock.lock().unwrap(),
            |delay| {
                let clock = clock.clone();
                async move {
                    *clock.lock().unwrap() += delay;
                }
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(run.report.attempts, 2);
    assert_eq!(run.report.stop, StopReason::Done);
    assert_eq!(run.report.total_waited, Duration::from_secs(1));
    run.outcome.unwrap().unwrap().decode().await.unwrap();
    assert_eq!(auth.application_attempts.load(Ordering::SeqCst), 2);
    assert_eq!(server.requests().len(), 2);
    assert_eq!(
        tokens.requests().len(),
        2,
        "separate token-service requests"
    );
    assert_eq!(server.requests()[0].body, server.requests()[1].body);
    assert_eq!(
        server.requests()[0].header("idempotency-key"),
        server.requests()[1].header("idempotency-key")
    );
    server.shutdown().await;
    tokens.shutdown().await;
}

#[tokio::test]
async fn synchronous_retry_receive_hook_reads_current_cache_after_backoff() {
    let calls = AtomicUsize::new(0);
    let server = TestServer::start(move |_| {
        Reply::new(if calls.fetch_add(1, Ordering::SeqCst) == 0 {
            503
        } else {
            200
        })
    })
    .await;
    let tokens = tokens().await;
    let auth = policy(&server, &tokens, true);
    auth.current().await.unwrap();
    let prepared = api(&server, auth.clone()).get().prepare().unwrap();
    let clock = std::cell::Cell::new(Duration::ZERO);
    let mut p = Policy::new(NonZeroUsize::new(2).unwrap());
    p.initial_delay = Duration::from_secs(1);
    p.max_delay = Duration::from_secs(1);
    let run = Retry::new(p, Replay::Permitted)
        .receive(
            prepared,
            ReplayMode::Required,
            |request, attempt| {
                assert_eq!(clock.get(), Duration::from_secs((attempt - 1) as u64));
                let current = auth.cache.get(&auth.scope).unwrap();
                request
                    .headers_mut()
                    .insert(AUTHORIZATION, current.header().clone());
            },
            |outcome| {
                if outcome
                    .as_ref()
                    .is_ok_and(|r| r.status() == StatusCode::SERVICE_UNAVAILABLE)
                {
                    Decision::RetryAfter(None)
                } else {
                    Decision::Done
                }
            },
            || clock.get(),
            |delay| {
                clock.set(clock.get() + delay);
                let auth = auth.clone();
                async move {
                    // Simulates a credential refresh completed by another request during backoff.
                    let previous = auth.cache.get(&auth.scope).unwrap();
                    auth.cache.invalidate(&auth.scope, &previous.generation());
                    auth.current().await.unwrap();
                }
            },
            |_, ceiling| ceiling,
        )
        .await;
    assert_eq!(run.report.attempts, 2);
    assert_eq!(run.report.stop, StopReason::Done);
    run.outcome.unwrap().unwrap().decode().await.unwrap();
    assert_eq!(
        server.requests()[0].header("authorization"),
        Some("Bearer token-1")
    );
    assert_eq!(
        server.requests()[1].header("authorization"),
        Some("Bearer token-2")
    );
    assert_eq!(tokens.requests().len(), 2);
    server.shutdown().await;
    tokens.shutdown().await;
}
