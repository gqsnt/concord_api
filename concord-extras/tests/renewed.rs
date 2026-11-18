#![cfg(feature = "auth")]
//! Renewal contracts, exercised with scripted HTTP and paused time.
// Test helpers use assertions and indexing just as the test bodies do.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::error::Class;
use concord::http::header::{HeaderName, HeaderValue};
use concord::page::Call as _;
use concord::{Base, Body, Retry, Secret};
use concord_extras::auth::{after_401, AccessToken, RenewalError, Renewed};
use concord_testkit::Script;
use futures_util::{future::join_all, FutureExt};
use serde::Deserialize;
use std::error::Error as _;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex, OnceLock,
};
use std::time::Duration;
use tokio::time::Instant;

// No Debug implementation: a token reply must never be formatted.
#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    expires_in: u64,
}

concord::api! {
    client DataApi {
        base "https://data.test";
        read concord::Text;
        GET get() -> String { path "data"; }
        POST upload(body: Body) -> String { path "data"; body body; }
        POST replace(body: Body) -> String { path "data"; body body; idempotent; }
        POST token() -> TokenReply { path "token"; read concord::Json; }
    }
}
concord::api! {
    client TokenApi {
        base "https://token.test";
        POST issue() -> TokenReply { path "token"; read concord::Json; }
    }
}

fn seconds(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn base() -> Base {
    "https://data.test".parse().unwrap()
}
fn initial(value: impl Into<Secret>, life: u64) -> AccessToken {
    AccessToken::expires_in(value, seconds(life)).unwrap()
}
fn renewed(script: &Script) -> Renewed {
    let client = TokenApi::new()
        .with_transport(script.clone())
        .with_timeout(seconds(6));
    Renewed::new(&base(), move || {
        let client = client.clone();
        async move {
            let reply = client.issue().await?;
            AccessToken::expires_in(reply.access_token, seconds(reply.expires_in))
        }
    })
    .early_by(seconds(1))
    .renewal_timeout(seconds(5))
}
fn mint(script: &Script, delay: u64) {
    script
        .answer(200)
        .after(seconds(delay))
        .body(r#"{"access_token":"new-secret","expires_in":100}"#);
}
fn client(script: &Script, auth: Renewed) -> DataApi {
    DataApi::new()
        .with_transport(script.clone())
        .with_middleware(auth)
}
fn assert_header(script: &Script, at: usize, name: &str, value: &str) {
    let requests = script.requests();
    let header = requests[at].headers().get(name).unwrap();
    assert!(header.to_str().unwrap() == value); // Do not print a credential on failure.
    assert!(header.is_sensitive());
    assert!(!format!("{header:?}").contains(value));
}
fn assert_attempts(script: &Script, attempts: &[u32]) {
    let requests = script.requests();
    assert_eq!(requests.len(), attempts.len());
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        attempts
    );
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_does_not_retry_other_client_errors() {
    for status in [403, 404, 429] {
        let tokens = Script::new();
        let api = Script::new();
        api.answer(status);
        api.answer(200).body("unused");
        let client = client(
            &api,
            renewed(&tokens)
                .with_token(initial("current-secret", 100))
                .unwrap(),
        )
        .with_retry(Retry::none().tries(2).when(after_401));

        let error = client.get().await.unwrap_err();
        assert!(error.is_status(status));
        assert_eq!(error.tries(), 1);
        assert_attempts(&api, &[1]);
        assert_attempts(&tokens, &[]);
        assert_eq!(api.unused(), 1);
    }
}
fn renewal(error: &concord::Error<()>) -> &RenewalError {
    error
        .source()
        .unwrap()
        .downcast_ref::<RenewalError>()
        .unwrap()
}
fn assert_redacted(error: &(dyn std::error::Error + 'static), secret: &str) {
    let mut next = Some(error);
    while let Some(error) = next {
        assert!(!format!("{error} {error:?}").contains(secret));
        next = error.source();
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_shares_one_bounded_flight_beyond_resource_idle_timeout() {
    let tokens = Script::new();
    mint(&tokens, 3);
    let api = Script::new();
    for _ in 0..12 {
        api.answer(200).body("ok");
    }
    let client = client(&api, renewed(&tokens)).with_idle_timeout(seconds(1));
    let start = Instant::now();
    assert!(join_all((0..12).map(|_| client.clone().get().send()))
        .await
        .into_iter()
        .all(|r| r.unwrap() == "ok"));
    assert_eq!(start.elapsed(), seconds(3));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1; 12]);
    for n in 0..12 {
        assert_header(&api, n, "authorization", "Bearer new-secret");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_expires_on_virtual_time_and_early_margin() {
    let tokens = Script::new();
    mint(&tokens, 1);
    let api = Script::new();
    for _ in 0..2 {
        api.answer(200).body("ok");
    }
    let token = initial("old-secret", 10);
    let client = client(&api, renewed(&tokens).with_token(token.clone()).unwrap());
    tokio::time::sleep(seconds(8)).await;
    assert_eq!(token.remaining(), seconds(2));
    client.get().await.unwrap();
    assert_attempts(&tokens, &[]);
    tokio::time::sleep(seconds(1)).await;
    client.get().await.unwrap();
    assert_eq!(token.remaining(), Duration::ZERO);
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1, 1]);
    assert_header(&api, 0, "authorization", "Bearer old-secret");
    assert_header(&api, 1, "authorization", "Bearer new-secret");
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_stale_401_keeps_new_generation() {
    let tokens = Script::new();
    mint(&tokens, 2);
    let api = Script::new();
    api.answer(401).after(seconds(10));
    api.answer(401);
    api.answer(200).body("B");
    api.answer(200).body("A");
    let client = client(
        &api,
        renewed(&tokens)
            .with_token(initial("old-secret", 100))
            .unwrap(),
    )
    .with_retry(Retry::none().tries(2).when(after_401));
    let (a, b) = tokio::join!(client.get().send(), client.get().send());
    assert_eq!((a.unwrap(), b.unwrap()), ("A".into(), "B".into()));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1, 1, 2, 2]);
    for (i, token) in ["old-secret", "old-secret", "new-secret", "new-secret"]
        .iter()
        .enumerate()
    {
        assert_header(&api, i, "authorization", &format!("Bearer {token}"));
    }
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_returns_401_unchanged_without_middleware_resend() {
    let tokens = Script::new();
    let api = Script::new();
    api.answer(401)
        .header("x-response", "kept")
        .body("original");
    let client = client(
        &api,
        renewed(&tokens)
            .with_token(initial("old-secret", 100))
            .unwrap(),
    )
    .with_retry(Retry::standard());
    let response = client.get().send_raw().await.unwrap();
    assert_eq!(response.status(), 401);
    assert_eq!(response.headers().get("x-response").unwrap(), "kept");
    assert_eq!(response.bytes().await.unwrap(), "original");
    assert_attempts(&api, &[1]);
    assert_attempts(&tokens, &[]);
    mint(&tokens, 0);
    api.answer(200).body("ok");
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_retries_only_through_engine_and_stops_after_second_401() {
    let tokens = Script::new();
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(401);
    api.answer(401);
    let client = client(
        &api,
        renewed(&tokens)
            .with_token(initial("old-secret", 100))
            .unwrap(),
    )
    .with_retry(Retry::standard().when(after_401));
    let error = client.get().await.unwrap_err();
    assert!(error.is_status(401));
    assert_eq!(error.tries(), 2);
    assert_attempts(&api, &[1, 2]);
    assert_attempts(&tokens, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_after_401_composes_with_policy_budget_and_veto() {
    for (statuses, policy, expected) in [
        (
            vec![401, 503, 200],
            Retry::standard().when(after_401),
            vec![1, 2, 3],
        ),
        (
            vec![503, 401],
            Retry::standard().when(after_401),
            vec![1, 2],
        ),
        (vec![401], Retry::none().when(after_401), vec![1]),
        (
            vec![401],
            Retry::standard().when(after_401).unless(|_| true),
            vec![1],
        ),
    ] {
        let tokens = Script::new();
        mint(&tokens, 0);
        let api = Script::new();
        for status in &statuses {
            api.answer(*status).body("ok");
        }
        let client = client(
            &api,
            renewed(&tokens)
                .with_token(initial("old-secret", 100))
                .unwrap(),
        )
        .with_retry(policy);
        let result = client.get().await;
        if statuses.last() == Some(&200) {
            assert_eq!(result.unwrap(), "ok");
        } else {
            let error = result.unwrap_err();
            assert!(error.is_status(401));
            assert_eq!(error.tries(), u32::try_from(expected.len()).unwrap());
        }
        assert_attempts(&api, &expected);
        assert_attempts(&tokens, if expected.len() == 3 { &[1] } else { &[] });
    }
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_post_requires_marking_or_not_applied_on() {
    for mode in 0..3 {
        let tokens = Script::new();
        mint(&tokens, 0);
        let api = Script::new();
        api.answer(401).read_upload();
        api.answer(200).read_upload().body("ok");
        let policy = Retry::none().tries(2).when(after_401);
        let policy = if mode == 2 {
            policy.not_applied_on(401)
        } else {
            policy
        };
        let client = client(
            &api,
            renewed(&tokens)
                .with_token(initial("old-secret", 100))
                .unwrap(),
        )
        .with_retry(policy);
        let call = if mode == 1 {
            client.replace(Body::from("payload")).send()
        } else {
            client.upload(Body::from("payload")).send()
        };
        let result = call.await;
        if mode == 0 {
            let error = result.unwrap_err();
            assert!(error.is_status(401));
            assert_eq!(error.tries(), 1);
            assert_attempts(&api, &[1]);
            assert_attempts(&tokens, &[]);
        } else {
            assert_eq!(result.unwrap(), "ok");
            assert_attempts(&api, &[1, 2]);
            assert_attempts(&tokens, &[1]);
            assert_eq!(
                api.requests()[0].upload().bytes(),
                api.requests()[1].upload().bytes()
            );
        }
        assert_eq!(api.requests()[0].upload().bytes(), b"payload");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_2_renewed_one_shot_body_is_never_resent() {
    let tokens = Script::new();
    let api = Script::new();
    api.answer(401).read_upload();
    let client = client(
        &api,
        renewed(&tokens)
            .with_token(initial("old-secret", 100))
            .unwrap(),
    )
    .with_retry(Retry::none().tries(2).not_applied_on(401).when(after_401));
    let body = Body::stream(futures_util::stream::iter([Ok::<_, std::io::Error>(
        concord::bytes::Bytes::from_static(b"upload"),
    )]));
    let error = client.replace(body).await.unwrap_err();
    assert!(error.is_status(401));
    assert_eq!(error.tries(), 1);
    assert_attempts(&api, &[1]);
    assert_attempts(&tokens, &[]);
    assert_eq!(api.requests()[0].upload().bytes(), b"upload");
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_failure_shares_and_preserves_token_endpoint_cause_then_tries_again() {
    let tokens = Script::new();
    tokens
        .answer(500)
        .after(seconds(2))
        .body("private-credential");
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let client = client(&api, renewed(&tokens)).with_retry(Retry::standard());
    let mut causes = Vec::new();
    for result in join_all((0..8).map(|_| client.get().send())).await {
        let error = result.unwrap_err();
        assert_eq!(error.class(), Some(Class::Blocked));
        assert_eq!(error.tries(), 1);
        assert_redacted(&error, "private-credential");
        let renewal = renewal(&error);
        let token_endpoint = renewal
            .source()
            .unwrap()
            .downcast_ref::<concord::Error<()>>()
            .unwrap();
        assert!(token_endpoint.is_status(500));
        assert_eq!(token_endpoint.tries(), 1);
        match renewal {
            RenewalError::Failed(cause) => causes.push(cause.clone()),
            _ => panic!("wrong failure"),
        }
    }
    assert!(causes.iter().all(|cause| Arc::ptr_eq(cause, &causes[0])));
    assert_attempts(&api, &[]);
    assert_attempts(&tokens, &[1]);
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1, 1]);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_timeout_is_shared_and_next_call_starts_another() {
    let tokens = Script::new();
    tokens.never();
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let client = client(&api, renewed(&tokens));
    let start = Instant::now();
    let errors = join_all((0..4).map(|_| client.get().send())).await;
    let mut causes = Vec::new();
    for error in errors {
        let error = error.unwrap_err();
        assert_eq!(error.tries(), 1);
        assert_redacted(&error, "secret");
        let error = renewal(&error);
        assert!(error.source().is_some());
        match error {
            RenewalError::TimedOut(cause) => causes.push(cause.clone()),
            _ => panic!("wrong failure"),
        }
    }
    assert!(causes.iter().all(|cause| Arc::ptr_eq(cause, &causes[0])));
    assert_eq!(start.elapsed(), seconds(5));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[]);
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1, 1]);
    assert_attempts(&api, &[1]);
}

struct Dropped(Arc<AtomicUsize>);
impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_last_drop_ends_renewal_and_next_call_starts_another() {
    let tokens = Script::new();
    mint(&tokens, 3);
    mint(&tokens, 3);
    let drops = Arc::new(AtomicUsize::new(0));
    let token_client = TokenApi::new().with_transport(tokens.clone());
    let auth = Renewed::new(&base(), {
        let drops = drops.clone();
        move || {
            let guard = Dropped(drops.clone());
            let token_client = token_client.clone();
            async move {
                let _guard = guard;
                let reply = token_client.issue().await?;
                AccessToken::expires_in(reply.access_token, seconds(reply.expires_in))
            }
        }
    });
    let api = Script::new();
    let client = client(&api, auth);
    let mut first = Box::pin(client.get().send());
    let mut second = Box::pin(client.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    drop(first);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(second);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let mut next = Box::pin(client.get().send());
    assert!(next.as_mut().now_or_never().is_none());
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(next);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_attempts(&api, &[]);
    assert_attempts(&tokens, &[1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_drop_leaves_other_waiters_and_no_parked_work() {
    let tokens = Script::new();
    mint(&tokens, 3);
    mint(&tokens, 3);
    let api = Script::new();
    api.answer(200).body("ok");
    api.answer(200).body("later");
    let client = client(&api, renewed(&tokens));
    let mut first = Box::pin(client.get().with_timeout(seconds(1)).send());
    let mut second = Box::pin(client.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    assert!(second.as_mut().now_or_never().is_none());
    let start = Instant::now();
    assert!(first.await.unwrap_err().is_timeout());
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[]);
    assert_eq!(second.await.unwrap(), "ok");
    assert_eq!(start.elapsed(), seconds(3));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1]);

    // Expire the cache, then time out the only waiter and abandon that token request.
    tokio::time::sleep(seconds(100)).await;
    assert!(client
        .get()
        .with_timeout(seconds(1))
        .await
        .unwrap_err()
        .is_timeout());
    mint(&tokens, 3);
    let start = Instant::now();
    assert_eq!(client.get().await.unwrap(), "later");
    assert_eq!(start.elapsed(), seconds(3));
    assert_attempts(&tokens, &[1, 1, 1]);
    assert_attempts(&api, &[1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_call_timeout_spans_401_renewal_and_second_transport_wait() {
    let tokens = Script::new();
    mint(&tokens, 1);
    let api = Script::new();
    // A one-second Retry-After fixes the wait at the jitter ceiling. The 401
    // and renewal finish at 2.5 s, leaving time to enter the second transport
    // before the call's 3 s deadline, for every permitted backoff draw.
    api.answer(401)
        .after(Duration::from_millis(500))
        .header("retry-after", "1");
    api.never();
    let client = client(
        &api,
        renewed(&tokens)
            .with_token(initial("old-secret", 100))
            .unwrap(),
    )
    .with_retry(Retry::none().tries(2).when(after_401));
    let start = Instant::now();
    let error = client
        .get()
        .with_timeout(seconds(3))
        .with_idle_timeout(seconds(10))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(error.tries(), 2);
    assert_eq!(start.elapsed(), seconds(3));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1, 2]);
}

#[test]
fn c11_1_renewed_debug_is_the_literal_redacted_name() {
    let tokens = Script::new();
    let auth = renewed(&tokens)
        .with_token(initial("PRIVATE_ACCESS_TOKEN", 100))
        .unwrap();
    assert_eq!(format!("{auth:?}"), "Renewed(..)");
    assert_attempts(&tokens, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_renews_at_exactly_the_early_margin() {
    let tokens = Script::new();
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let token = initial("old-secret", 10);
    let auth = renewed(&tokens).early_by(seconds(2));
    let client = client(&api, auth.with_token(token.clone()).unwrap());
    tokio::time::advance(seconds(8)).await;
    assert_eq!(token.remaining(), seconds(2));
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1]);
    assert_header(&api, 0, "authorization", "Bearer new-secret");
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_resource_idle_timeout_starts_after_renewal() {
    let tokens = Script::new();
    mint(&tokens, 3);
    let api = Script::new();
    api.never();
    let client = client(&api, renewed(&tokens));
    let start = Instant::now();
    let error = client
        .get()
        .with_idle_timeout(seconds(1))
        .await
        .unwrap_err();
    assert!(error.is_timeout());
    assert_eq!(error.tries(), 1);
    assert_eq!(start.elapsed(), seconds(4));
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_rejects_other_origin_before_renewal() {
    for origin in [
        "http://data.test",
        "https://other.test",
        "https://data.test:444",
    ] {
        let tokens = Script::new();
        let api = Script::new();
        let client = client(&api, renewed(&tokens)).with_base(origin.parse::<Base>().unwrap());
        let error = client.get().await.unwrap_err();
        assert!(matches!(renewal(&error), RenewalError::WrongOrigin));
        assert_eq!(error.tries(), 1);
        assert_redacted(&error, "data.test");
        assert_attempts(&tokens, &[]);
        assert_attempts(&api, &[]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_base_prefix_scopes_only_origin_including_default_port() {
    let tokens = Script::new();
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let tokens_client = TokenApi::new().with_transport(tokens.clone());
    let auth = Renewed::new(
        &"https://DATA.test:443/v2".parse::<Base>().unwrap(),
        move || {
            let client = tokens_client.clone();
            async move {
                let reply = client.issue().await?;
                AccessToken::expires_in(reply.access_token, seconds(reply.expires_in))
            }
        },
    );
    let client = client(&api, auth);
    assert_eq!(
        client
            .get()
            .at_url("https://data.test/sibling")
            .await
            .unwrap(),
        "ok"
    );
    assert_eq!(api.requests()[0].url(), "https://data.test/sibling");
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_custom_header_is_sensitive_and_seed_rechecks_final_configuration() {
    let tokens = Script::new();
    let api = Script::new();
    api.answer(200).body("ok");
    let auth = renewed(&tokens)
        .with_token(initial("old-secret", 100))
        .unwrap()
        .header(HeaderName::from_static("x-api-key"), |secret| {
            HeaderValue::from_str(&secret.expose())
        });
    let client = client(&api, auth);
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_header(&api, 0, "x-api-key", "old-secret");
    assert!(api.requests()[0].headers().get("authorization").is_none());
    assert_attempts(&tokens, &[]);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_snapshots_secret_aliases_and_notification_input() {
    let secret = Secret::new("old-secret");
    let alias = secret.clone();
    let tokens = Script::new();
    let api = Script::new();
    for _ in 0..2 {
        api.answer(200).body("ok");
    }
    let auth = renewed(&tokens).with_token(initial(secret, 100)).unwrap();
    alias.replace("replacement-secret");
    let client = client(&api, auth);
    client.get().await.unwrap();
    assert_header(&api, 0, "authorization", "Bearer old-secret");
    assert_attempts(&tokens, &[]);

    let secret = Secret::new("new-secret");
    let alias = secret.clone();
    let auth = Renewed::new(&base(), move || {
        let secret = secret.clone();
        async move { Ok(initial(secret, 100)) }
    })
    .on_renewed(|token| token.value().replace("notification-secret"));
    let renewed_client = DataApi::new()
        .with_transport(api.clone())
        .with_middleware(auth);
    renewed_client.get().await.unwrap();
    alias.replace("later-secret");
    api.answer(200).body("ok");
    renewed_client.get().await.unwrap();
    assert_header(&api, 1, "authorization", "Bearer new-secret");
    assert_header(&api, 2, "authorization", "Bearer new-secret");
    assert_attempts(&api, &[1, 1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_notification_precedes_publication_once_and_seed_never_notifies() {
    let tokens = Script::new();
    mint(&tokens, 2);
    let api = Script::new();
    for _ in 0..5 {
        api.answer(200).body("ok");
    }
    let notifications = Arc::new(AtomicUsize::new(0));
    let auth = renewed(&tokens).on_renewed({
        let notifications = notifications.clone();
        let api = api.clone();
        move |_| {
            assert!(api.requests().is_empty());
            notifications.fetch_add(1, Ordering::SeqCst);
        }
    });
    let client = client(&api, auth);
    let results = join_all((0..4).map(|_| {
        let notifications = notifications.clone();
        let call = client.get().send();
        async move {
            let result = call.await;
            assert_eq!(notifications.load(Ordering::SeqCst), 1);
            result
        }
    }))
    .await;
    assert!(results.into_iter().all(|r| r.is_ok()));
    client.get().await.unwrap();
    assert_eq!(notifications.load(Ordering::SeqCst), 1);
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1; 5]);
    let auth = renewed(&tokens)
        .on_renewed(|_| panic!("seed must not notify"))
        .with_token(initial("seed", 100))
        .unwrap();
    api.answer(200).body("seeded");
    let seeded = DataApi::new()
        .with_transport(api.clone())
        .with_middleware(auth);
    assert_eq!(seeded.get().await.unwrap(), "seeded");
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1; 6]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_panics_fail_all_waiters_without_hanging_and_can_try_again() {
    // Exercise panic both when the callback is invoked and when its future is polled.
    for synchronous in [false, true] {
        let calls = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new(tokio::sync::Notify::new());
        let auth = Renewed::new(&base(), {
            let calls = calls.clone();
            let gate = gate.clone();
            move || {
                let number = calls.fetch_add(1, Ordering::SeqCst);
                assert!(!(synchronous && number == 0), "private-panic-payload");
                let gate = gate.clone();
                async move {
                    if number == 0 {
                        gate.notified().await;
                        panic!("private-panic-payload");
                    }
                    Ok(initial("new-secret", 100))
                }
            }
        });
        let api = Script::new();
        api.answer(200).body("ok");
        let client = client(&api, auth);
        let mut first = Box::pin(client.get().send());
        if synchronous {
            let error = first.await.unwrap_err();
            assert!(matches!(renewal(&error), RenewalError::Panicked));
            assert!(renewal(&error).source().is_none());
            assert_redacted(&error, "private-panic-payload");
        } else {
            let mut second = Box::pin(client.get().send());
            assert!(first.as_mut().now_or_never().is_none());
            assert!(second.as_mut().now_or_never().is_none());
            gate.notify_one();
            let (first, second) = tokio::join!(first, second);
            for error in [first.unwrap_err(), second.unwrap_err()] {
                assert!(matches!(renewal(&error), RenewalError::Panicked));
                assert!(renewal(&error).source().is_none());
                assert_redacted(&error, "private-panic-payload");
                assert_eq!(error.tries(), 1);
            }
        }
        assert_attempts(&api, &[]);
        assert_eq!(client.get().await.unwrap(), "ok");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_attempts(&api, &[1]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_never_prints_secrets_on_header_decoder_and_notification_failures() {
    for (body, expected) in [
        (
            r#"{"access_token":"bad\nprivate-credential","expires_in":100}"#,
            "header",
        ),
        (
            r#"{"access_token":"private-credential","expires_in":0}"#,
            "expiry",
        ),
        (r#"{"expires_in":"private-credential"}"#, "decode"),
    ] {
        let tokens = Script::new();
        tokens.answer(200).body(body);
        let api = Script::new();
        let error = client(&api, renewed(&tokens)).get().await.unwrap_err();
        assert_redacted(&error, "private-credential");
        match expected {
            "header" => {
                let error = renewal(&error);
                assert!(matches!(error, RenewalError::InvalidHeader(_)));
                assert!(error
                    .source()
                    .unwrap()
                    .is::<concord::http::header::InvalidHeaderValue>());
            }
            "expiry" => assert!(matches!(renewal(&error), RenewalError::Expired)),
            _ => assert!(matches!(renewal(&error), RenewalError::Failed(_))),
        }
        assert_attempts(&tokens, &[1]);
        assert_attempts(&api, &[]);
    }
    for header_panic in [false, true] {
        let tokens = Script::new();
        mint(&tokens, 2);
        let api = Script::new();
        let auth = if header_panic {
            renewed(&tokens).header(HeaderName::from_static("x-secret"), |_| {
                panic!("private-credential")
            })
        } else {
            renewed(&tokens).on_renewed(|_| panic!("private-credential"))
        };
        assert!(!format!("{auth:?}").contains("secret"));
        let client = client(&api, auth);
        let errors = join_all((0..3).map(|_| client.get().send())).await;
        for error in errors {
            let error = error.unwrap_err();
            assert!(matches!(renewal(&error), RenewalError::Panicked));
            assert_redacted(&error, "private-credential");
        }
        assert_attempts(&tokens, &[1]);
        assert_attempts(&api, &[]);
    }
    assert_eq!(
        format!("{:?}", initial("private-credential", 100)),
        "AccessToken(..)"
    );
    for error in [
        RenewalError::Expired,
        RenewalError::Panicked,
        RenewalError::WrongOrigin,
        RenewalError::InvalidPolicy,
        RenewalError::TimeOverflow,
    ] {
        assert_redacted(&error, "private-credential");
        assert!(error.source().is_none());
    }
    // Explicit source access retains an author's own error content by design.
    let error = RenewalError::failed(std::io::Error::other("author-owned-secret"));
    assert!(!format!("{error} {error:?}").contains("author-owned-secret"));
    assert!(error
        .source()
        .unwrap()
        .to_string()
        .contains("author-owned-secret"));
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_validates_seeds_timeout_and_checked_deadlines() {
    assert!(matches!(
        AccessToken::expires_in("zero", Duration::ZERO),
        Err(RenewalError::Expired)
    ));
    assert!(matches!(
        AccessToken::expires_in("overflow", Duration::MAX),
        Err(RenewalError::TimeOverflow)
    ));
    let tokens = Script::new();
    let api = Script::new();
    let auth = renewed(&tokens)
        .with_token(initial("bad\nheader", 100))
        .unwrap();
    let error = client(&api, auth).get().await.unwrap_err();
    assert!(matches!(renewal(&error), RenewalError::InvalidHeader(_)));
    assert_attempts(&api, &[]);
    let expired = initial("expired", 1);
    tokio::time::sleep(seconds(1)).await;
    assert!(matches!(
        renewed(&tokens).with_token(expired),
        Err(RenewalError::Expired)
    ));
    for (auth, overflow) in [
        (renewed(&tokens).renewal_timeout(Duration::ZERO), false),
        (renewed(&tokens).renewal_timeout(Duration::MAX), true),
    ] {
        let api = Script::new();
        let error = client(&api, auth).get().await.unwrap_err();
        if overflow {
            assert!(matches!(renewal(&error), RenewalError::TimeOverflow));
        } else {
            assert!(matches!(renewal(&error), RenewalError::InvalidPolicy));
        }
        assert_attempts(&api, &[]);
    }
    assert_attempts(&tokens, &[]);
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let auth = renewed(&tokens)
        .with_token(initial("seed", 10))
        .unwrap()
        .early_by(seconds(20));
    assert_eq!(client(&api, auth).get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_retains_only_current_token_and_one_flight() {
    struct Live(Arc<AtomicUsize>);
    impl Drop for Live {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    struct Cause {
        _live: Live,
    }
    impl std::fmt::Debug for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Cause")
        }
    }
    impl std::fmt::Display for Cause {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Cause")
        }
    }
    impl std::error::Error for Cause {}
    struct HeaderOwner {
        bytes: Vec<u8>,
        _live: Live,
    }
    impl AsRef<[u8]> for HeaderOwner {
        fn as_ref(&self) -> &[u8] {
            &self.bytes
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let futures = Arc::new(AtomicUsize::new(0));
    let causes = Arc::new(AtomicUsize::new(0));
    let headers = Arc::new(AtomicUsize::new(0));
    let auth = Renewed::new(&base(), {
        let futures = futures.clone();
        let causes = causes.clone();
        let calls = calls.clone();
        move || {
            assert_eq!(futures.fetch_add(1, Ordering::SeqCst), 0);
            let guard = Live(futures.clone());
            let causes = causes.clone();
            let number = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                let _guard = guard;
                tokio::time::sleep(seconds(1)).await;
                if number.is_multiple_of(2) {
                    assert_eq!(causes.fetch_add(1, Ordering::SeqCst), 0);
                    Err(RenewalError::failed(Cause {
                        _live: Live(causes),
                    }))
                } else {
                    Ok(initial("new-secret", 100))
                }
            }
        }
    })
    .header(HeaderName::from_static("x-token"), {
        let headers = headers.clone();
        move |secret| {
            assert_eq!(headers.fetch_add(1, Ordering::SeqCst), 0);
            HeaderValue::from_maybe_shared(concord::bytes::Bytes::from_owner(HeaderOwner {
                bytes: secret.expose().as_bytes().to_vec(),
                _live: Live(headers.clone()),
            }))
        }
    });
    for _ in 0..20 {
        let api = Script::new();
        let client = client(&api, auth.clone());
        let error = client.get().await.unwrap_err();
        assert!(matches!(renewal(&error), RenewalError::Failed(_)));
        assert_eq!(causes.load(Ordering::SeqCst), 1);
        drop(error);
        assert_eq!(causes.load(Ordering::SeqCst), 0);
        assert_eq!(futures.load(Ordering::SeqCst), 0);
        assert_eq!(headers.load(Ordering::SeqCst), 0);
        assert_attempts(&api, &[]);
        api.answer(200).body("ok");
        assert_eq!(client.get().await.unwrap(), "ok");
        assert_eq!(futures.load(Ordering::SeqCst), 0);
        assert_attempts(&api, &[1]);
        drop(client);
        drop(api); // Release the test transport's copy of the header.
        assert_eq!(headers.load(Ordering::SeqCst), 1); // Only the current cache entry.
        tokio::time::sleep(seconds(100)).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 40);
    drop(auth);
    assert_eq!(headers.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_direct_recursive_wait_is_refused_before_sending() {
    let holder = Arc::new(OnceLock::<DataApi>::new());
    let weak = Arc::downgrade(&holder);
    let auth = Renewed::new(&base(), move || {
        let client = weak.upgrade().unwrap().get().unwrap().clone();
        async move {
            let reply = client.token().await?;
            AccessToken::expires_in(reply.access_token, seconds(reply.expires_in))
        }
    })
    .renewal_timeout(seconds(3));
    let api = Script::new();
    let client = client(&api, auth);
    assert!(holder.set(client.clone()).is_ok());
    let start = Instant::now();
    let error = client.get().await.unwrap_err();
    let token_error = renewal(&error)
        .source()
        .unwrap()
        .downcast_ref::<concord::Error<()>>()
        .unwrap();
    assert!(matches!(renewal(token_error), RenewalError::InvalidPolicy));
    assert_eq!(start.elapsed(), Duration::ZERO);
    assert_attempts(&api, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_default_timeout_is_thirty_seconds_and_drops_work() {
    let calls = Arc::new(AtomicUsize::new(0));
    let drops = Arc::new(AtomicUsize::new(0));
    let auth = Renewed::new(&base(), {
        let calls = calls.clone();
        let drops = drops.clone();
        move || {
            let number = calls.fetch_add(1, Ordering::SeqCst);
            let guard = Dropped(drops.clone());
            async move {
                let _guard = guard;
                if number == 0 {
                    std::future::pending::<()>().await;
                }
                Ok(initial("new-secret", 100))
            }
        }
    });
    let api = Script::new();
    let client = client(&api, auth);
    let start = Instant::now();
    for error in join_all((0..5).map(|_| client.get().send())).await {
        let error = error.unwrap_err();
        assert!(matches!(renewal(&error), RenewalError::TimedOut(_)));
        assert_eq!(error.tries(), 1);
    }
    assert_eq!(start.elapsed(), seconds(30));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_attempts(&api, &[]);
    api.answer(200).body("ok");
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    assert_attempts(&api, &[1]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_clones_share_seed_and_configuration() {
    let tokens = Script::new();
    let auth = renewed(&tokens);
    let configured = auth
        .clone()
        .header(HeaderName::from_static("x-token"), |token| {
            HeaderValue::from_str(&token.expose())
        })
        .with_token(initial("seed-secret", 100))
        .unwrap();
    let api = Script::new();
    api.answer(200).body("ok");
    assert_eq!(client(&api, auth).get().await.unwrap(), "ok");
    assert_header(&api, 0, "x-token", "seed-secret");
    assert_attempts(&api, &[1]);
    assert_attempts(&tokens, &[]);
    drop(configured);
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_invalid_final_header_configuration_preserves_safe_cause() {
    let tokens = Script::new();
    let api = Script::new();
    let auth = renewed(&tokens)
        .with_token(initial("seed-secret", 100))
        .unwrap()
        .header(HeaderName::from_static("x-token"), |_| {
            HeaderValue::from_str("invalid\nprivate-credential")
        });
    let error = client(&api, auth).get().await.unwrap_err();
    assert!(matches!(renewal(&error), RenewalError::InvalidHeader(_)));
    assert_redacted(&error, "private-credential");
    assert!(renewal(&error)
        .source()
        .unwrap()
        .is::<concord::http::header::InvalidHeaderValue>());
    assert_eq!(error.tries(), 1);
    assert_attempts(&api, &[]);
    assert_attempts(&tokens, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_1_renewed_full_header_map_fails_without_panicking_or_sending() {
    let tokens = Script::new();
    let api = Script::new();
    let auth = renewed(&tokens)
        .with_token(initial("seed-secret", 100))
        .unwrap();
    let error = client(&api, auth)
        .get()
        .edit_request(|request| {
            for number in 0..32768 {
                let name = HeaderName::from_bytes(format!("x-fill-{number}").as_bytes()).unwrap();
                if request
                    .headers_mut()
                    .try_insert(name, HeaderValue::from_static("value"))
                    .is_err()
                {
                    return;
                }
            }
            panic!("the header map must reach its bound");
        })
        .await
        .unwrap_err();
    assert!(matches!(renewal(&error), RenewalError::Failed(_)));
    assert!(renewal(&error)
        .source()
        .unwrap()
        .is::<concord::http::header::MaxSizeReached>());
    assert_redacted(&error, "seed-secret");
    assert_eq!(error.tries(), 1);
    assert_attempts(&api, &[]);
    assert_attempts(&tokens, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_accepts_seeds_inside_margin_in_either_builder_order() {
    for margin_first in [false, true] {
        for margin in [seconds(10), seconds(20)] {
            let tokens = Script::new();
            mint(&tokens, 0);
            let api = Script::new();
            api.answer(200).body("ok");
            let notifications = Arc::new(AtomicUsize::new(0));
            let auth = renewed(&tokens).on_renewed({
                let notifications = notifications.clone();
                move |_| {
                    notifications.fetch_add(1, Ordering::SeqCst);
                }
            });
            let seed = initial("seed-secret", 10);
            let auth = if margin_first {
                auth.early_by(margin).with_token(seed).unwrap()
            } else {
                auth.with_token(seed).unwrap().early_by(margin)
            };
            assert_eq!(notifications.load(Ordering::SeqCst), 0);
            assert_attempts(&tokens, &[]);
            assert_attempts(&api, &[]);
            assert_eq!(client(&api, auth).get().await.unwrap(), "ok");
            assert_eq!(notifications.load(Ordering::SeqCst), 1);
            assert_header(&api, 0, "authorization", "Bearer new-secret");
            assert_attempts(&tokens, &[1]);
            assert_attempts(&api, &[1]);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_seed_acceptance_defers_header_and_policy_checks_in_either_order() {
    for configure_first in [false, true] {
        for invalid_header in [false, true] {
            let tokens = Script::new();
            let api = Script::new();
            let formatted = Arc::new(AtomicUsize::new(0));
            let configure = |auth: Renewed| {
                if invalid_header {
                    let formatted = formatted.clone();
                    auth.header(HeaderName::from_static("x-token"), move |_| {
                        formatted.fetch_add(1, Ordering::SeqCst);
                        HeaderValue::from_str("invalid\nprivate-credential")
                    })
                } else {
                    auth.renewal_timeout(Duration::ZERO)
                }
            };
            let auth = renewed(&tokens);
            let seed = initial("seed-secret", 100);
            let auth = if configure_first {
                configure(auth).with_token(seed).unwrap()
            } else {
                configure(auth.with_token(seed).unwrap())
            };
            assert_eq!(formatted.load(Ordering::SeqCst), 0);
            let error = client(&api, auth).get().await.unwrap_err();
            if invalid_header {
                assert!(matches!(renewal(&error), RenewalError::InvalidHeader(_)));
                assert_eq!(formatted.load(Ordering::SeqCst), 1);
            } else {
                assert!(matches!(renewal(&error), RenewalError::InvalidPolicy));
            }
            assert_redacted(&error, "private-credential");
            assert_eq!(error.tries(), 1);
            assert_attempts(&tokens, &[]);
            assert_attempts(&api, &[]);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_expired_seeds_are_refused_regardless_of_builder_order() {
    for configure_first in [false, true] {
        let tokens = Script::new();
        let expired = initial("seed-secret", 1);
        tokio::time::sleep(seconds(1)).await;
        let auth = renewed(&tokens);
        let result = if configure_first {
            auth.early_by(seconds(10))
                .renewal_timeout(Duration::ZERO)
                .with_token(expired)
        } else {
            auth.with_token(expired)
                .map(|auth| auth.early_by(seconds(10)).renewal_timeout(Duration::ZERO))
        };
        assert!(matches!(result, Err(RenewalError::Expired)));
        assert_attempts(&tokens, &[]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_short_renewal_tokens_fail_all_waiters_and_allow_another_renewal() {
    for lifetime in [9, 10] {
        let tokens = Script::new();
        tokens.answer(200).after(seconds(2)).body(format!(
            "{{\"access_token\":\"short-secret\",\"expires_in\":{lifetime}}}"
        ));
        mint(&tokens, 0);
        let api = Script::new();
        let notifications = Arc::new(AtomicUsize::new(0));
        let auth = renewed(&tokens).early_by(seconds(10)).on_renewed({
            let notifications = notifications.clone();
            move |_| {
                notifications.fetch_add(1, Ordering::SeqCst);
            }
        });
        let client = client(&api, auth);
        for result in join_all((0..3).map(|_| client.get().send())).await {
            let error = result.unwrap_err();
            assert!(matches!(renewal(&error), RenewalError::Expired));
            assert!(renewal(&error).source().is_none());
            assert_eq!(error.tries(), 1);
        }
        assert_eq!(notifications.load(Ordering::SeqCst), 0);
        assert_attempts(&tokens, &[1]);
        assert_attempts(&api, &[]);
        api.answer(200).body("ok");
        assert_eq!(client.get().await.unwrap(), "ok");
        assert_eq!(notifications.load(Ordering::SeqCst), 1);
        assert_header(&api, 0, "authorization", "Bearer new-secret");
        assert_attempts(&tokens, &[1, 1]);
        assert_attempts(&api, &[1]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_late_waiter_renews_before_sending_an_expired_token() {
    let tokens = Script::new();
    tokens
        .answer(200)
        .after(seconds(1))
        .body(r#"{"access_token":"short-secret","expires_in":1}"#);
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("first");
    api.answer(200).body("second");
    let client = client(&api, renewed(&tokens).early_by(Duration::ZERO));
    let mut first = Box::pin(client.get().send());
    let mut late = Box::pin(client.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    assert!(late.as_mut().now_or_never().is_none());
    tokio::time::advance(seconds(1)).await;
    assert_eq!(first.await.unwrap(), "first");
    tokio::time::advance(seconds(2)).await;
    assert_eq!(late.await.unwrap(), "second");
    assert_attempts(&tokens, &[1, 1]);
    assert_attempts(&api, &[1, 1]);
    assert_header(&api, 0, "authorization", "Bearer short-secret");
    assert_header(&api, 1, "authorization", "Bearer new-secret");
}

#[tokio::test(start_paused = true)]
async fn c11_3_elapsed_deadline_wins_over_ready_renewal_work() {
    let tokens = Script::new();
    mint(&tokens, 2);
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("ok");
    let notified = Arc::new(AtomicUsize::new(0));
    let auth = renewed(&tokens).renewal_timeout(seconds(1)).on_renewed({
        let notified = notified.clone();
        move |_| {
            notified.fetch_add(1, Ordering::SeqCst);
        }
    });
    let client = client(&api, auth);
    let mut call = Box::pin(client.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    tokio::time::advance(seconds(3)).await;
    let result = call.await;
    assert_eq!(
        notified.load(Ordering::SeqCst),
        0,
        "renewal notified after its deadline"
    );
    let error = result.unwrap_err();
    assert!(matches!(renewal(&error), RenewalError::TimedOut(_)));
    assert_eq!(error.tries(), 1);
    assert_attempts(&api, &[]);
    assert_attempts(&tokens, &[1]);
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_eq!(notified.load(Ordering::SeqCst), 1);
    assert_attempts(&api, &[1]);
    assert_attempts(&tokens, &[1, 1]);
}

#[tokio::test(start_paused = true)]
async fn c11_2_late_waiter_does_not_reuse_a_rejected_shared_token() {
    let tokens = Script::new();
    mint(&tokens, 1);
    tokens
        .answer(200)
        .body(r#"{"access_token":"fresh-secret","expires_in":100}"#);
    let api = Script::new();
    api.answer(401);
    api.answer(200).body("ok");
    let client = client(&api, renewed(&tokens));
    let mut first = Box::pin(client.get().send());
    let mut late = Box::pin(client.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    assert!(late.as_mut().now_or_never().is_none());
    tokio::time::advance(seconds(1)).await;
    let error = first.await.unwrap_err();
    assert!(error.is_status(401));
    assert_eq!(error.tries(), 1);
    assert_eq!(late.await.unwrap(), "ok");
    assert_attempts(&tokens, &[1, 1]);
    assert_attempts(&api, &[1, 1]);
    assert_header(&api, 1, "authorization", "Bearer fresh-secret");
}

#[tokio::test(start_paused = true)]
async fn c11_3_late_waiter_rechecks_the_early_margin() {
    let tokens = Script::new();
    tokens
        .answer(200)
        .after(seconds(1))
        .body(r#"{"access_token":"short-secret","expires_in":5}"#);
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("first");
    api.answer(200).body("second");
    let client = client(&api, renewed(&tokens).early_by(seconds(2)));
    let mut first = Box::pin(client.get().send());
    let mut late = Box::pin(client.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    assert!(late.as_mut().now_or_never().is_none());
    tokio::time::advance(seconds(1)).await;
    assert_eq!(first.await.unwrap(), "first");
    tokio::time::advance(seconds(3)).await;
    assert_eq!(late.await.unwrap(), "second");
    assert_attempts(&tokens, &[1, 1]);
    assert_attempts(&api, &[1, 1]);
    assert_header(&api, 1, "authorization", "Bearer new-secret");
}

#[tokio::test(start_paused = true)]
async fn c11_3_deadline_is_rechecked_after_author_callbacks() {
    for in_formatter in [false, true] {
        let tokens = Script::new();
        mint(&tokens, 0);
        mint(&tokens, 0);
        let api = Script::new();
        api.answer(200).body("ok");
        let invoked = Arc::new(AtomicUsize::new(0));
        let notified = Arc::new(AtomicUsize::new(0));
        let auth = renewed(&tokens)
            .renewal_timeout(seconds(1))
            .header(HeaderName::from_static("x-token"), {
                let invoked = invoked.clone();
                move |token| {
                    if in_formatter && invoked.fetch_add(1, Ordering::SeqCst) == 0 {
                        // advance changes virtual time before yielding; no real-time wait.
                        let _ = tokio::time::advance(seconds(2)).now_or_never();
                    }
                    HeaderValue::from_str(&token.expose())
                }
            })
            .on_renewed({
                let invoked = invoked.clone();
                let notified = notified.clone();
                move |_| {
                    notified.fetch_add(1, Ordering::SeqCst);
                    if !in_formatter && invoked.fetch_add(1, Ordering::SeqCst) == 0 {
                        let _ = tokio::time::advance(seconds(2)).now_or_never();
                    }
                }
            });
        let client = client(&api, auth);
        let error = client.get().await.unwrap_err();
        assert!(matches!(renewal(&error), RenewalError::TimedOut(_)));
        assert_eq!(notified.load(Ordering::SeqCst), usize::from(!in_formatter));
        assert_attempts(&api, &[]);
        assert_attempts(&tokens, &[1]);
        assert_eq!(client.get().await.unwrap(), "ok");
        assert_eq!(
            notified.load(Ordering::SeqCst),
            usize::from(!in_formatter) + 1
        );
        assert_attempts(&api, &[1]);
        assert_attempts(&tokens, &[1, 1]);
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_deadline_wins_at_the_exact_boundary() {
    let tokens = Script::new();
    mint(&tokens, 1);
    let api = Script::new();
    let client = client(
        &api,
        renewed(&tokens)
            .renewal_timeout(seconds(1))
            .on_renewed(|_| panic!("a token arriving at the deadline must not notify")),
    );
    let mut call = Box::pin(client.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    tokio::time::advance(seconds(1)).await;
    let error = call.await.unwrap_err();
    assert!(matches!(renewal(&error), RenewalError::TimedOut(_)));
    assert_eq!(error.tries(), 1);
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewal_error_find_reaches_the_typed_error_and_its_cause() {
    struct Wrapped(concord::Error<()>);
    impl std::fmt::Debug for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("Wrapped")
        }
    }
    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("the call failed")
        }
    }
    impl std::error::Error for Wrapped {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    let tokens = Script::new();
    tokens.answer(503).body("private-token-endpoint-response");
    let api = Script::new();
    let error = client(&api, renewed(&tokens)).get().await.unwrap_err();
    let wrapped = Wrapped(error);
    let found = RenewalError::find(&wrapped).unwrap();
    assert!(matches!(found, RenewalError::Failed(_)));
    assert!(std::ptr::eq(found, RenewalError::find(&wrapped.0).unwrap()));
    assert!(std::ptr::eq(found, RenewalError::find(found).unwrap()));
    let cause = found
        .source()
        .unwrap()
        .downcast_ref::<concord::Error<()>>()
        .unwrap();
    assert!(cause.is_status(503));
    assert_eq!(cause.tries(), 1);
    assert!(RenewalError::find(cause).is_none());
    assert!(RenewalError::find(&std::io::Error::other("unrelated")).is_none());
    assert_redacted(&wrapped, "private-token-endpoint-response");
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[]);
}

#[test]
fn c11_1_renewal_errors_display_sentences_without_values() {
    for (error, message) in [
        (
            RenewalError::failed(std::io::Error::other("private-cause")),
            "the credential could not be renewed or installed",
        ),
        (
            RenewalError::TimedOut(Arc::new(std::io::Error::other("private-cause"))),
            "the credential renewal exceeded renewal_timeout",
        ),
        (RenewalError::Panicked, "a credential callback panicked"),
        (
            RenewalError::InvalidHeader(Arc::new(
                HeaderValue::from_str("invalid\nprivate-cause").unwrap_err(),
            )),
            "the credential could not be formatted as a header",
        ),
        (
            RenewalError::Expired,
            "the credential has expired or does not outlast early_by",
        ),
        (
            RenewalError::WrongOrigin,
            "the request leaves the credential's origin",
        ),
        (
            RenewalError::InvalidPolicy,
            "the credential renewal policy is invalid",
        ),
        (
            RenewalError::TimeOverflow,
            "the credential deadline cannot be represented",
        ),
    ] {
        assert_eq!(error.to_string(), message);
        assert!(!format!("{error} {error:?}").contains("private-cause"));
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_header_retirement_rechecks_lifetime_before_sending() {
    use concord::reqwest::{Request, Response};
    use concord::transport::{Transport, TransportError, TryInfo};

    struct ExpiresOnDrop(Arc<AtomicUsize>);
    impl AsRef<[u8]> for ExpiresOnDrop {
        fn as_ref(&self) -> &[u8] {
            b"Bearer seed-secret"
        }
    }
    impl Drop for ExpiresOnDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
            let _ = tokio::time::advance(seconds(2)).now_or_never();
        }
    }
    #[derive(Clone)]
    struct StripsHeader {
        script: Script,
        sent: Arc<Mutex<Vec<String>>>,
    }
    impl Transport for StripsHeader {
        async fn send(
            &self,
            mut request: Request,
            info: TryInfo,
        ) -> Result<Response, TransportError> {
            let header = request.headers_mut().remove("authorization").unwrap();
            self.sent
                .lock()
                .unwrap()
                .push(header.to_str().unwrap().to_owned());
            drop(header); // The cache must own the last reference before retirement.
            self.script.send(request, info).await
        }
    }
    let tokens = Script::new();
    mint(&tokens, 0);
    let api = Script::new();
    api.answer(200).body("inner");
    api.answer(200).body("outer");
    let holder = Arc::new(OnceLock::<DataApi>::new());
    let weak = Arc::downgrade(&holder);
    let formats = Arc::new(AtomicUsize::new(0));
    let retired = Arc::new(AtomicUsize::new(0));
    let auth = renewed(&tokens)
        .early_by(Duration::ZERO)
        .header(HeaderName::from_static("authorization"), {
            let retired = retired.clone();
            let formats = formats.clone();
            move |token| {
                match formats.fetch_add(1, Ordering::SeqCst) {
                    0 => {
                        // Finish B while A's formatter is running. B installs
                        // the header whose owner A will retire after preparing.
                        let holder = weak.upgrade().unwrap();
                        let result = holder.get().unwrap().get().send().now_or_never();
                        assert_eq!(result.unwrap().unwrap(), "inner");
                    }
                    1 => {
                        return HeaderValue::from_maybe_shared(concord::bytes::Bytes::from_owner(
                            ExpiresOnDrop(retired.clone()),
                        ))
                    }
                    _ => {}
                }
                HeaderValue::from_str(&format!("Bearer {}", &*token.expose()))
            }
        })
        .with_token(initial("seed-secret", 1))
        .unwrap();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let transport = StripsHeader {
        script: api.clone(),
        sent: sent.clone(),
    };
    let client = DataApi::new()
        .with_transport(transport)
        .with_middleware(auth);
    assert!(holder.set(client.clone()).is_ok());
    let start = Instant::now();
    assert_eq!(client.get().await.unwrap(), "outer");
    assert_eq!(retired.load(Ordering::SeqCst), 1);
    assert_eq!(start.elapsed(), seconds(2));
    let sent = sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert!(sent[0] == "Bearer seed-secret");
    assert!(
        sent[1] == "Bearer new-secret",
        "a header destructor made an expired seed reach the transport"
    );
    assert_attempts(&tokens, &[1]);
    assert_attempts(&api, &[1, 1]);
    assert_eq!(api.requests()[0].at() - start, Duration::ZERO);
    assert_eq!(api.requests()[1].at() - start, seconds(2));
}

#[test]
fn c11_3_renewal_error_find_stops_at_a_self_cycle() {
    #[derive(Debug)]
    struct Cycle(std::cell::Cell<usize>);
    impl std::fmt::Display for Cycle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("cyclic source")
        }
    }
    impl std::error::Error for Cycle {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            let calls = self.0.get();
            assert!(calls < 64, "find repeatedly followed a self-cyclic source");
            self.0.set(calls + 1);
            Some(self)
        }
    }
    let error = Cycle(std::cell::Cell::new(0));
    assert!(RenewalError::find(&error).is_none());
    #[cfg(feature = "limit")]
    assert!(concord_extras::limit::LimitError::find(&error).is_none());
}

#[test]
fn c11_3_renewal_error_find_stops_at_a_longer_cycle() {
    #[derive(Debug)]
    struct Cycle {
        next: OnceLock<Arc<Cycle>>,
        calls: Arc<AtomicUsize>,
    }
    impl std::fmt::Display for Cycle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("cyclic source")
        }
    }
    impl std::error::Error for Cycle {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            assert!(
                self.calls.fetch_add(1, Ordering::SeqCst) < 64,
                "find repeatedly followed the same cycle of source errors"
            );
            self.next.get().map(|next| next.as_ref() as _)
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let a = Arc::new(Cycle {
        next: OnceLock::new(),
        calls: calls.clone(),
    });
    let b = Arc::new(Cycle {
        next: OnceLock::new(),
        calls: calls.clone(),
    });
    let c = Arc::new(Cycle {
        next: OnceLock::new(),
        calls: calls.clone(),
    });
    a.next.set(b.clone()).unwrap();
    b.next.set(c.clone()).unwrap();
    c.next.set(a.clone()).unwrap();
    assert!(RenewalError::find(a.as_ref()).is_none());
    #[cfg(feature = "limit")]
    assert!(concord_extras::limit::LimitError::find(a.as_ref()).is_none());
}

#[test]
fn c11_3_renewal_error_find_finds_a_renewal_error_within_a_source_cycle() {
    #[derive(Debug)]
    struct Link(OnceLock<Arc<dyn std::error::Error + Send + Sync>>);
    impl std::fmt::Display for Link {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("source link")
        }
    }
    impl std::error::Error for Link {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            self.0.get().map(|next| next.as_ref() as _)
        }
    }
    let a = Arc::new(Link(OnceLock::new()));
    let b = Arc::new(Link(OnceLock::new()));
    let renewal = Arc::new(RenewalError::Failed(a.clone()));
    a.0.set(b.clone()).unwrap();
    b.0.set(renewal.clone()).unwrap();
    let prefix = Link(OnceLock::new());
    prefix.0.set(a).unwrap();
    assert!(std::ptr::eq(
        RenewalError::find(&prefix).unwrap(),
        renewal.as_ref()
    ));
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewed_treats_a_renewal_not_sent_as_a_failure_and_tries_again() {
    let tokens = Script::new();
    tokens.fail(concord::transport::TransportError::blocked());
    mint(&tokens, 0);
    let token_client = TokenApi::new().with_transport(tokens.clone());
    let auth = Renewed::new(&base(), move || {
        let token_client = token_client.clone();
        async move {
            let reply = token_client
                .issue()
                .await
                .map_err(RenewalError::from_call)?;
            AccessToken::expires_in(reply.access_token, seconds(reply.expires_in))
        }
    });
    let api = Script::new();
    api.answer(200).body("ok");
    let client = client(&api, auth);
    let error = client.get().await.unwrap_err();
    assert_eq!(error.class(), Some(Class::Blocked));
    assert!(matches!(renewal(&error), RenewalError::NotSent(_)));
    assert_attempts(&api, &[]);
    assert_eq!(client.get().await.unwrap(), "ok");
    assert_attempts(&tokens, &[1, 1]);
    assert_header(&api, 0, "authorization", "Bearer new-secret");
}
