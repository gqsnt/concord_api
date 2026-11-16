//! C11.1 to C11.3: the renewed credential, `concord::auth` (feature `auth`).
//!
//! `Renewed`, `AccessToken`,
//! `RenewalError` and `after_401`, attached to a client of `concord::api!` over a
//! `concord_testkit::Script`, on paused `tokio` time.

mod common;

use concord::auth::{after_401, AccessToken, RenewalError, Renewed};
use concord::error::{Class, Kind};
use concord::http::{HeaderName, HeaderValue};
use concord::{Base, Retry};
use concord_testkit::Script;
use std::error::Error as _;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::{sleep, timeout, Instant};

concord::api! {
    client Docs {
        base "https://docs.example.test/v2";
        read concord::Text;
        GET title() -> String { path "title"; }
        POST note(text: String) -> String {
            path "notes";
            body concord::Body::new(text.clone(), "text/plain");
        }
        POST marked(text: String) -> String {
            path "marked";
            idempotent;
            body concord::Body::new(text.clone(), "text/plain");
        }
        PUT upload(file: concord::Body) -> String {
            path "upload";
            body file.clone();
        }
    }
}

concord::api! {
    client Tokens {
        base "https://identity.example.test";
        read concord::Text;
        POST issue() -> String {
            path "token";
            body concord::Body::new("grant_type=client_credentials", "application/x-www-form-urlencoded");
        }
    }
}

// What the tests share ----------------------------------------------------------------

/// The text that every token holds, so that a printed line can be searched for it.
const MARK: &str = "s3cr3t";

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

/// The base of `Docs`, given to `Renewed::new`.
fn base() -> Base {
    "https://docs.example.test/v2"
        .parse()
        .expect("the base parses")
}

/// `Renewed::new` on the base of these tests.
fn renewed<F, Fut>(renew: F) -> Renewed
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<AccessToken, RenewalError>> + Send + 'static,
{
    Renewed::new(&base(), renew)
}

/// Counts the futures of a renewal that are alive.
struct Live(Arc<AtomicUsize>);

impl Live {
    fn new(count: &Arc<AtomicUsize>) -> Live {
        count.fetch_add(1, Ordering::SeqCst);
        Live(count.clone())
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// An author's renewal: the `n`-th renewal waits its delay (the last one given, for the
/// later ones) and returns `s3cr3t-<n>`, valid for an hour.
#[derive(Clone)]
struct Issuer {
    count: Arc<AtomicUsize>,
    live: Arc<AtomicUsize>,
    delays: Arc<Vec<Duration>>,
    lifetime: Duration,
}

impl Issuer {
    fn new() -> Issuer {
        Issuer::with_delays(&[Duration::ZERO])
    }

    fn with_delays(delays: &[Duration]) -> Issuer {
        Issuer {
            count: Arc::default(),
            live: Arc::default(),
            delays: Arc::new(delays.to_vec()),
            lifetime: secs(3600),
        }
    }

    fn lifetime(mut self, lifetime: Duration) -> Issuer {
        self.lifetime = lifetime;
        self
    }

    /// How many renewals were started.
    fn count(&self) -> usize {
        self.count.load(Ordering::SeqCst)
    }

    /// How many renewal futures are alive.
    fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    fn renewed(&self) -> Renewed {
        let issuer = self.clone();
        renewed(move || {
            let n = issuer.count.fetch_add(1, Ordering::SeqCst) + 1;
            let delay = issuer
                .delays
                .get(n - 1)
                .or(issuer.delays.last())
                .copied()
                .unwrap_or_default();
            let lifetime = issuer.lifetime;
            let live = Live::new(&issuer.live);
            async move {
                let _live = live;
                if !delay.is_zero() {
                    sleep(delay).await;
                }
                AccessToken::expires_in(format!("{MARK}-{n}"), lifetime)
            }
        })
    }
}

fn token(value: &str) -> AccessToken {
    AccessToken::expires_in(value, secs(3600)).expect("a token of an hour")
}

fn docs(script: &Script, auth: Renewed) -> Docs {
    Docs::new()
        .with_middleware(auth)
        .with_transport(script.clone())
}

fn answer(script: &Script, status: u16, body: &'static str) {
    script.answer(status).body(body);
}

/// The `authorization` header of every request that reached the script.
fn authorizations(script: &Script) -> Vec<Option<String>> {
    script
        .requests()
        .iter()
        .map(|request| request.header("authorization").map(str::to_owned))
        .collect()
}

fn bearer(n: usize) -> Option<String> {
    Some(format!("Bearer {MARK}-{n}"))
}

#[track_caller]
fn success<T, E>(result: Result<T, concord::Error<E>>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!(
            "expected a value, got an error {:?} (class {:?}, status {:?})",
            error.kind(),
            error.class(),
            error.status()
        ),
    }
}

#[track_caller]
fn failure<T, E>(result: Result<T, concord::Error<E>>) -> concord::Error<E> {
    match result {
        Ok(_) => panic!("expected an error, got a value"),
        Err(error) => error,
    }
}

/// The renewal error that a call ended with: "a resource call exposes this error directly as
/// its transport source".
#[track_caller]
fn renewal_error(error: &concord::Error) -> &RenewalError {
    assert_eq!(error.kind(), Kind::Transport, "the kind of the error");
    assert_eq!(
        error.class(),
        Some(Class::Blocked),
        "the class of the error"
    );
    error
        .source()
        .and_then(|source| source.downcast_ref::<RenewalError>())
        .expect("the source of the error is a RenewalError")
}

/// Everything an error prints: its line, its `Debug`, and the same for each of its sources.
fn printed(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = format!("{error}\n{error:?}");
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!("\n{cause}\n{cause:?}"));
        source = cause.source();
    }
    text
}

#[track_caller]
fn prints_no_token(text: &str) {
    assert!(!text.contains(MARK), "a printed text holds the token");
}

// C11.1 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_1_the_token_goes_as_bearer_and_is_marked_sensitive() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();

    assert_eq!(
        success(docs(&script, issuer.renewed()).title().await),
        "title"
    );

    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    let values: Vec<_> = requests[0]
        .headers()
        .get_all("authorization")
        .iter()
        .collect();
    assert_eq!(values.len(), 1, "one authorization header");
    assert_eq!(values[0].as_bytes(), format!("Bearer {MARK}-1").as_bytes());
    assert!(values[0].is_sensitive(), "the header is marked sensitive");
}

#[tokio::test(start_paused = true)]
async fn c11_1_a_header_of_the_authors_is_marked_sensitive_and_replaces_bearer() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();
    let auth = issuer
        .renewed()
        .header(HeaderName::from_static("x-api-key"), |token| {
            HeaderValue::from_str(&token.expose())
        });

    success(docs(&script, auth).title().await);

    let requests = script.requests();
    assert_eq!(requests[0].header("x-api-key"), Some("s3cr3t-1"));
    assert!(requests[0]
        .headers()
        .get("x-api-key")
        .is_some_and(HeaderValue::is_sensitive));
    assert_eq!(
        requests[0].header("authorization"),
        None,
        "no Bearer beside it"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_1_another_origin_is_blocked_before_any_renewal() {
    for other in [
        "https://other.example.test/v2",
        "https://api.docs.example.test/v2",
        "http://docs.example.test/v2",
        "https://docs.example.test:8443/v2",
    ] {
        let script = Script::new();
        answer(&script, 200, "title");
        let issuer = Issuer::new();

        let error = failure(
            docs(&script, issuer.renewed())
                .with_base(other)
                .title()
                .await,
        );

        assert!(
            matches!(renewal_error(&error), RenewalError::WrongOrigin),
            "{other}: WrongOrigin"
        );
        assert_eq!(issuer.count(), 0, "{other}: no renewal");
        assert_eq!(script.requests().len(), 0, "{other}: nothing sent");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_1_the_url_that_is_sent_is_checked_not_the_base_of_the_client() {
    // `edit_request` moves the request after it was built: the middleware sees the real URL.
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();
    let auth = issuer
        .renewed()
        .with_token(token("s3cr3t-seed"))
        .expect("the seed is valid");

    let error = failure(
        docs(&script, auth)
            .title()
            .edit_request(|request| {
                *request.url_mut() = "https://elsewhere.example.test/v2/title"
                    .parse()
                    .expect("a URL");
            })
            .await,
    );

    assert!(matches!(renewal_error(&error), RenewalError::WrongOrigin));
    assert_eq!(script.requests().len(), 0, "nothing sent");
    assert_eq!(issuer.count(), 0, "no renewal");
}

#[tokio::test(start_paused = true)]
async fn c11_1_a_path_prefix_neither_narrows_nor_widens_the_origin() {
    // The same origin, written differently or with another path: the token goes.
    for same in [
        "https://docs.example.test/other",
        "https://docs.example.test",
        "https://docs.example.test:443/v2",
        "https://DOCS.example.test/v2",
    ] {
        let script = Script::new();
        answer(&script, 200, "title");
        let issuer = Issuer::new();

        success(
            docs(&script, issuer.renewed())
                .with_base(same)
                .title()
                .await,
        );

        assert_eq!(authorizations(&script), vec![bearer(1)], "{same}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_1_debug_prints_no_token() {
    let seed = token("s3cr3t-seed");
    prints_no_token(&format!("{seed:?}"));

    let auth = Issuer::new()
        .renewed()
        .early_by(secs(20))
        .renewal_timeout(secs(6))
        .header(HeaderName::from_static("x-api-key"), |token| {
            HeaderValue::from_str(&token.expose())
        })
        .with_token(seed.clone())
        .expect("the seed is valid");
    prints_no_token(&format!("{auth:?}"));
    prints_no_token(&format!("{:?}", auth.clone()));

    // After use, with a token in the cache.
    let script = Script::new();
    answer(&script, 200, "title");
    let client = docs(&script, auth.clone());
    success(client.title().await);
    prints_no_token(&format!("{auth:?}"));
    prints_no_token(&format!("{client:?}"));
}

#[tokio::test(start_paused = true)]
async fn c11_1_a_token_that_cannot_be_a_header_value_is_not_printed() {
    // The default `Bearer` header, and a token with a line feed in it.
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = renewed(|| async { AccessToken::expires_in("s3cr3t\nline", secs(3600)) });

    let error = failure(docs(&script, auth).title().await);

    assert!(matches!(
        renewal_error(&error),
        RenewalError::InvalidHeader(_)
    ));
    prints_no_token(&printed(&error));
    assert_eq!(script.requests().len(), 0, "nothing sent");
}

#[tokio::test(start_paused = true)]
async fn c11_1_the_formatting_error_of_the_authors_header_is_kept_and_not_printed() {
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = Issuer::new()
        .renewed()
        .header(HeaderName::from_static("x-api-key"), |token| {
            HeaderValue::from_str(&format!("{}\r\n", &*token.expose()))
        });

    let error = failure(docs(&script, auth).title().await);

    let renewal = renewal_error(&error);
    assert!(matches!(renewal, RenewalError::InvalidHeader(_)));
    assert!(
        renewal
            .source()
            .is_some_and(|cause| cause.is::<concord::http::header::InvalidHeaderValue>()),
        "the standard error is the source"
    );
    prints_no_token(&printed(&error));
    assert_eq!(script.requests().len(), 0, "nothing sent");
}

#[tokio::test(start_paused = true)]
async fn c11_1_a_token_endpoint_that_fails_prints_nothing_of_its_body() {
    let tokens_script = Script::new();
    tokens_script
        .answer(400)
        .header("www-authenticate", "Basic s3cr3t-in-a-header")
        .body("s3cr3t-in-the-body");
    let tokens = Tokens::new().with_transport(tokens_script.clone());
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = renewed(move || {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.issue().await?;
            AccessToken::expires_in(reply, secs(3600))
        }
    });

    let error = failure(docs(&script, auth).title().await);

    assert!(matches!(renewal_error(&error), RenewalError::Failed(_)));
    prints_no_token(&printed(&error));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_1_a_renewal_that_panics_prints_nothing_of_its_payload() {
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = renewed(|| async {
        if MARK.len() > 1 {
            panic!("{MARK}-in-a-panic");
        }
        AccessToken::expires_in("never", secs(3600))
    });

    let error = failure(docs(&script, auth).title().await);

    assert!(matches!(renewal_error(&error), RenewalError::Panicked));
    prints_no_token(&printed(&error));
    assert_eq!(script.requests().len(), 0);
}

// C11.2 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_2_a_401_is_returned_as_it_is_and_sent_once() {
    let script = Script::new();
    script
        .answer(401)
        .header("www-authenticate", "Bearer error=\"invalid_token\"")
        .body("denied");
    answer(&script, 200, "title");
    let issuer = Issuer::new();

    let error = failure(docs(&script, issuer.renewed()).title().await);

    assert!(error.is_status(401));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.body(), Some(&b"denied"[..]));
    assert_eq!(
        error
            .headers()
            .and_then(|headers| headers.get("www-authenticate")),
        Some("Bearer error=\"invalid_token\"")
    );
    assert_eq!(
        script.requests().len(),
        1,
        "the middleware sends nothing again"
    );
    assert_eq!(script.unused(), 1);
    assert_eq!(issuer.count(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_2_a_401_rejects_the_token_and_the_next_call_renews() {
    let script = Script::new();
    answer(&script, 200, "one");
    answer(&script, 401, "denied");
    answer(&script, 200, "two");
    answer(&script, 200, "three");
    let issuer = Issuer::new();
    let client = docs(&script, issuer.renewed());

    success(client.title().await);
    assert!(failure(client.title().await).is_status(401));
    success(client.title().await);
    success(client.title().await);

    assert_eq!(
        authorizations(&script),
        vec![bearer(1), bearer(1), bearer(2), bearer(2)]
    );
    assert_eq!(issuer.count(), 2);
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_gives_a_get_one_more_attempt_with_a_new_token() {
    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 200, "title");
    let issuer = Issuer::new();

    let title = success(
        docs(&script, issuer.renewed())
            .with_retry(Retry::none().tries(2).when(after_401))
            .title()
            .await,
    );

    assert_eq!(title, "title");
    let requests = script.requests();
    let attempts: Vec<u32> = requests
        .iter()
        .map(|request| request.info().number())
        .collect();
    assert_eq!(attempts, vec![1, 2]);
    assert_eq!(authorizations(&script), vec![bearer(1), bearer(2)]);
    assert_eq!(issuer.count(), 2);
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_is_worth_an_attempt_only_after_the_first() {
    // A budget of five tries: a second 401 is final.
    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 401, "denied again");
    answer(&script, 200, "title");
    let issuer = Issuer::new();

    let error = failure(
        docs(&script, issuer.renewed())
            .with_retry(Retry::none().tries(5).when(after_401))
            .title()
            .await,
    );

    assert!(error.is_status(401));
    assert_eq!(error.tries(), 2);
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.unused(), 1);

    // A 401 on the second attempt of the standard policy, after a 503: no third.
    let script = Script::new();
    answer(&script, 503, "busy");
    answer(&script, 401, "denied");
    answer(&script, 200, "title");

    let error = failure(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::standard().when(after_401))
            .title()
            .await,
    );

    assert!(error.is_status(401));
    assert_eq!(script.requests().len(), 2);
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_is_worth_an_attempt_after_a_401_only() {
    // A 403, a 404 and a 429 on the first attempt: `after_401` adds no attempt, under a
    // policy that has no other rule and a budget of five tries. The token is not rejected.
    for status in [403, 404, 429] {
        let script = Script::new();
        script.answer(status).header("retry-after", "1").body("no");
        answer(&script, 200, "title");
        answer(&script, 200, "title");
        let issuer = Issuer::new();
        let client = docs(&script, issuer.renewed());

        let error = failure(
            client
                .title()
                .with_retry(Retry::none().tries(5).when(after_401))
                .await,
        );

        assert!(error.is_status(status), "{status}");
        assert_eq!(error.tries(), 1, "{status}");
        assert_eq!(script.requests().len(), 1, "{status}: sent once");
        success(client.title().await);
        assert_eq!(
            authorizations(&script),
            vec![bearer(1), bearer(1)],
            "{status}: the token is kept"
        );
        assert_eq!(issuer.count(), 1, "{status}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_keeps_the_waits_of_the_policy() {
    // The wait before the next attempt is the longest of the rule's (zero), the response's
    // `Retry-After`, and a random part of at most 1 s before the second attempt.
    let script = Script::new();
    script.answer(401).header("retry-after", "3").body("denied");
    answer(&script, 200, "title");
    let start = Instant::now();
    success(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::none().tries(2).when(after_401))
            .title()
            .await,
    );
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].at() - start,
        secs(3),
        "the Retry-After is waited"
    );

    // The random part, drawn anew for each wait, is at most 1 s before the second attempt.
    // The surface gives no way to fix the draw, so this is a statistical check: over 20
    // calls, every wait is within 1 s, at least one is not zero, and not all are equal. A
    // random part replaced by zero, or drawn once, fails it.
    let mut waits = Vec::new();
    for _ in 0..20 {
        let script = Script::new();
        answer(&script, 401, "denied");
        answer(&script, 200, "title");
        let start = Instant::now();
        success(
            docs(&script, Issuer::new().renewed())
                .with_retry(Retry::standard().when(after_401))
                .title()
                .await,
        );
        let requests = script.requests();
        assert_eq!(requests.len(), 2);
        waits.push(requests[1].at() - start);
    }
    assert!(
        waits.iter().all(|wait| *wait <= secs(1)),
        "at most 1 s: {waits:?}"
    );
    assert!(
        waits.iter().any(|wait| !wait.is_zero()),
        "a random part is waited: {waits:?}"
    );
    assert!(
        waits.iter().any(|wait| *wait != waits[0]),
        "drawn anew for each wait: {waits:?}"
    );

    // A wait over 60 s, or one that would end after the call's timeout, is no attempt.
    for (retry_after, timeout) in [("120", None), ("5", Some(secs(3)))] {
        let script = Script::new();
        script
            .answer(401)
            .header("retry-after", retry_after)
            .body("denied");
        answer(&script, 200, "title");
        let mut call = docs(&script, Issuer::new().renewed())
            .with_retry(Retry::none().tries(2).when(after_401))
            .title();
        if let Some(timeout) = timeout {
            call = call.with_timeout(timeout);
        }
        let error = failure(call.await);
        assert!(error.is_status(401), "retry-after {retry_after}");
        assert_eq!(script.requests().len(), 1, "retry-after {retry_after}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_2_after_401_keeps_the_rules_of_the_policy() {
    // `standard` still retries a 503.
    let script = Script::new();
    answer(&script, 503, "busy");
    answer(&script, 200, "title");

    success(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::standard().when(after_401))
            .title()
            .await,
    );
    assert_eq!(script.requests().len(), 2);

    // And `none` with one try makes one, after a 401 too.
    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 200, "title");

    let error = failure(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::none().when(after_401))
            .title()
            .await,
    );
    assert!(error.is_status(401));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_2_an_unmarked_post_is_not_sent_again_after_a_401() {
    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 200, "noted");
    let issuer = Issuer::new();

    let error = failure(
        docs(&script, issuer.renewed())
            .with_retry(Retry::standard().when(after_401))
            .note("text".to_owned())
            .await,
    );

    assert!(error.is_status(401));
    assert_eq!(script.requests().len(), 1);
    assert_eq!(issuer.count(), 1, "nothing renews on its own");
}

#[tokio::test(start_paused = true)]
async fn c11_2_a_marked_post_and_a_post_under_not_applied_on_go_again() {
    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 200, "marked");

    success(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::standard().when(after_401))
            .marked("text".to_owned())
            .await,
    );
    assert_eq!(authorizations(&script), vec![bearer(1), bearer(2)]);
    let bodies: Vec<_> = script
        .requests()
        .iter()
        .map(|request| request.body().map(<[u8]>::to_vec))
        .collect();
    assert_eq!(bodies, vec![Some(b"text".to_vec()), Some(b"text".to_vec())]);

    let script = Script::new();
    answer(&script, 401, "denied");
    answer(&script, 200, "noted");

    success(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::standard().when(after_401).not_applied_on(401))
            .note("text".to_owned())
            .await,
    );
    assert_eq!(authorizations(&script), vec![bearer(1), bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_2_a_body_that_is_a_stream_is_not_sent_again() {
    // A PUT may be repeated by its method; its body may not.
    let script = Script::new();
    script.answer(401).read_upload().body("denied");
    answer(&script, 200, "uploaded");

    let error = failure(
        docs(&script, Issuer::new().renewed())
            .with_retry(Retry::standard().when(after_401))
            .upload(common::stream("the bytes"))
            .await,
    );

    assert!(error.is_status(401));
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.requests()[0].upload().bytes(), b"the bytes");
}

#[tokio::test(start_paused = true)]
async fn c11_2_a_late_401_rejects_only_the_token_it_carried() {
    // A carries the seed and gets its 401 at 10 s. Meanwhile B carries the seed too, gets a
    // 401 at once, and C renews. A's late 401 must not reject C's token: D uses it as it is.
    let script = Script::new();
    script.answer(401).after(secs(10)).body("late"); // A
    answer(&script, 401, "denied"); // B
    answer(&script, 200, "c"); // C
    answer(&script, 200, "d"); // D
    let issuer = Issuer::new();
    let auth = issuer
        .renewed()
        .with_token(token("s3cr3t-0"))
        .expect("the seed is valid");
    let client = docs(&script, auth);

    let (a, (b, c)) = tokio::join!(client.title().send(), async {
        sleep(ms(1)).await;
        let b = client.title().send().await;
        let c = client.title().send().await;
        (b, c)
    });
    let d = client.title().send().await;

    assert!(failure(a).is_status(401));
    assert!(failure(b).is_status(401));
    assert_eq!(success(c), "c");
    assert_eq!(success(d), "d");
    assert_eq!(
        authorizations(&script),
        vec![bearer(0), bearer(0), bearer(1), bearer(1)]
    );
    assert_eq!(issuer.count(), 1);
}

// C11.3 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c11_3_the_calls_that_wait_share_one_renewal_and_its_token() {
    let script = Script::new();
    for _ in 0..3 {
        answer(&script, 200, "title");
    }
    let issuer = Issuer::with_delays(&[secs(2)]);
    let client = docs(&script, issuer.renewed());
    let start = Instant::now();

    let (a, b, c) = tokio::join!(
        client.title().send(),
        client.title().send(),
        client.title().send()
    );

    success(a);
    success(b);
    success(c);
    assert_eq!(issuer.count(), 1);
    assert_eq!(authorizations(&script), vec![bearer(1); 3]);
    for request in script.requests() {
        assert_eq!(request.at() - start, secs(2), "sent when the renewal ends");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_the_calls_that_wait_share_the_failure_and_its_cause() {
    let script = Script::new();
    answer(&script, 200, "title");
    let count = Arc::new(AtomicUsize::new(0));
    let counted = count.clone();
    let auth = renewed(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        async {
            sleep(secs(1)).await;
            Err(RenewalError::failed(std::io::Error::other(
                "the token endpoint is down",
            )))
        }
    });
    let client = docs(&script, auth);

    let (a, b) = tokio::join!(client.title().send(), client.title().send());

    for error in [failure(a), failure(b)] {
        let renewal = renewal_error(&error);
        assert!(matches!(renewal, RenewalError::Failed(_)));
        let cause = renewal.source().expect("the cause is kept");
        assert_eq!(cause.to_string(), "the token endpoint is down");
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_3_the_error_of_the_token_endpoint_is_the_cause() {
    let tokens_script = Script::new();
    tokens_script.answer(500).body("down");
    let tokens = Tokens::new().with_transport(tokens_script.clone());
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = renewed(move || {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.issue().await?;
            AccessToken::expires_in(reply, secs(3600))
        }
    });

    let error = failure(docs(&script, auth).title().await);

    let renewal = renewal_error(&error);
    assert!(matches!(renewal, RenewalError::Failed(_)));
    let cause = renewal
        .source()
        .and_then(|cause| cause.downcast_ref::<concord::Error>())
        .expect("the cause is the error of the token call");
    assert!(cause.is_status(500));
    assert_eq!(cause.endpoint(), "tokens.issue");
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_failed_renewal_lets_the_next_call_try_again() {
    let script = Script::new();
    answer(&script, 200, "title");
    let count = Arc::new(AtomicUsize::new(0));
    let counted = count.clone();
    let auth = renewed(move || {
        let n = counted.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            if n == 1 {
                Err(RenewalError::failed(std::io::Error::other("once")))
            } else {
                AccessToken::expires_in(format!("{MARK}-{n}"), secs(3600))
            }
        }
    });
    let client = docs(&script, auth);

    failure(client.title().await);
    success(client.title().await);

    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(authorizations(&script), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewal_timeout_ends_the_renewal_and_the_next_call_tries_again() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::with_delays(&[secs(3600), Duration::ZERO]);
    let client = docs(&script, issuer.renewed().renewal_timeout(secs(5)));
    let start = Instant::now();

    let error = failure(client.title().await);

    assert_eq!(start.elapsed(), secs(5));
    assert!(matches!(renewal_error(&error), RenewalError::TimedOut(_)));
    assert_eq!(issuer.live(), 0, "the renewal that timed out is gone");

    success(client.title().await);
    assert_eq!(issuer.count(), 2);
    assert_eq!(authorizations(&script), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_the_renewal_timeout_is_30_seconds_by_default() {
    let script = Script::new();
    let issuer = Issuer::with_delays(&[secs(3600)]);
    let start = Instant::now();

    let error = failure(docs(&script, issuer.renewed()).title().await);

    assert_eq!(start.elapsed(), secs(30));
    assert!(matches!(renewal_error(&error), RenewalError::TimedOut(_)));
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_renewal_timeout_of_zero_is_refused_before_renewal() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();

    let error = failure(
        docs(&script, issuer.renewed().renewal_timeout(Duration::ZERO))
            .title()
            .await,
    );

    assert!(matches!(renewal_error(&error), RenewalError::InvalidPolicy));
    assert_eq!(issuer.count(), 0);
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_dropped_call_leaves_the_renewal_to_the_others() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::with_delays(&[secs(4)]);
    let client = docs(&script, issuer.renewed());
    let start = Instant::now();

    let (dropped, kept) = tokio::join!(
        timeout(secs(1), client.title().send()),
        client.title().send()
    );

    assert!(dropped.is_err(), "the first call was dropped at 1 s");
    success(kept);
    assert_eq!(issuer.count(), 1, "one renewal, not restarted by the drop");
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].at() - start, secs(4));
    assert_eq!(authorizations(&script), vec![bearer(1)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_the_last_drop_ends_the_renewal_and_the_next_call_starts_one() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::with_delays(&[secs(10)]);
    let client = docs(&script, issuer.renewed());
    let start = Instant::now();

    let (a, b) = tokio::join!(
        timeout(secs(1), client.title().send()),
        timeout(secs(2), client.title().send())
    );
    assert!(a.is_err() && b.is_err(), "both calls were dropped");
    assert_eq!(issuer.live(), 0, "the renewal ended with the last drop");
    assert_eq!(issuer.count(), 1);

    success(client.title().await);

    assert_eq!(issuer.count(), 2, "a new renewal");
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].at() - start, secs(2) + secs(10));
    assert_eq!(authorizations(&script), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_no_task_is_started() {
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::with_delays(&[secs(5)]);
    let client = docs(&script, issuer.renewed());
    let metrics = tokio::runtime::Handle::current().metrics();
    let before = metrics.num_alive_tasks();

    let (result, during) = tokio::join!(client.title().send(), async {
        sleep(secs(1)).await;
        (issuer.live(), metrics.num_alive_tasks())
    });

    success(result);
    assert_eq!(during.0, 1, "the renewal is in flight at 1 s");
    assert_eq!(during.1, before, "and no task runs it");
    assert_eq!(metrics.num_alive_tasks(), before);
}

#[tokio::test(start_paused = true)]
async fn c11_3_early_by_renews_before_the_token_expires() {
    let script = Script::new();
    for _ in 0..3 {
        answer(&script, 200, "title");
    }
    let issuer = Issuer::new().lifetime(secs(60));
    let client = docs(&script, issuer.renewed().early_by(secs(20)));

    success(client.title().await); // at 0 s: renews
    sleep(secs(39)).await;
    success(client.title().await); // 21 s left: the token is kept
    sleep(secs(2)).await;
    success(client.title().await); // 19 s left: renews

    assert_eq!(
        authorizations(&script),
        vec![bearer(1), bearer(1), bearer(2)]
    );
    assert_eq!(issuer.count(), 2);
}

#[tokio::test(start_paused = true)]
async fn c11_3_an_expired_token_is_renewed_and_a_seed_counts_as_the_first() {
    let script = Script::new();
    answer(&script, 200, "seeded");
    answer(&script, 200, "renewed");
    let issuer = Issuer::new();
    let seed = AccessToken::expires_in("s3cr3t-0", secs(10)).expect("a token of 10 s");
    let client = docs(&script, issuer.renewed().with_token(seed).expect("valid"));

    success(client.title().await);
    sleep(secs(10)).await;
    success(client.title().await);

    assert_eq!(authorizations(&script), vec![bearer(0), bearer(1)]);
    assert_eq!(issuer.count(), 1);
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_token_that_does_not_outlast_early_by_is_refused() {
    assert!(matches!(
        AccessToken::expires_in("s3cr3t", Duration::ZERO),
        Err(RenewalError::Expired)
    ));

    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new().lifetime(secs(30));

    let error = failure(
        docs(&script, issuer.renewed().early_by(secs(60)))
            .title()
            .await,
    );

    assert!(matches!(renewal_error(&error), RenewalError::Expired));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c11_3_on_renewed_hears_each_token_once_before_it_is_used() {
    let script = Script::new();
    for _ in 0..3 {
        answer(&script, 200, "title");
    }
    let heard: Arc<Mutex<Vec<(String, usize)>>> = Arc::default();
    let issuer = Issuer::with_delays(&[secs(1)]);
    let (log, seen) = (heard.clone(), script.clone());
    let auth = issuer.renewed().on_renewed(move |token| {
        let sent = seen.requests().len();
        log.lock()
            .expect("the log")
            .push((token.value().expose().to_string(), sent));
    });
    let client = docs(&script, auth);

    let (a, b, c) = tokio::join!(
        client.title().send(),
        client.title().send(),
        client.title().send()
    );
    success(a);
    success(b);
    success(c);

    assert_eq!(
        *heard.lock().expect("the log"),
        vec![("s3cr3t-1".to_owned(), 0)],
        "once, before any request carried it"
    );
}

#[tokio::test(start_paused = true)]
async fn c11_3_the_remaining_lifetime_follows_the_timer() {
    let token = AccessToken::expires_in("s3cr3t", secs(300)).expect("a token");
    assert_eq!(token.remaining(), secs(300));
    sleep(secs(100)).await;
    assert_eq!(token.remaining(), secs(200));
    sleep(secs(250)).await;
    assert_eq!(token.remaining(), Duration::ZERO, "saturates at zero");
    assert_eq!(&*token.value().expose(), "s3cr3t");

    assert!(matches!(
        AccessToken::expires_in("s3cr3t", Duration::MAX),
        Err(RenewalError::TimeOverflow)
    ));
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_renewal_dropped_mid_way_is_not_counted_as_a_token() {
    // Dropping the only caller of a renewal that was about to succeed publishes nothing:
    // `on_renewed` hears nothing, and the next call renews.
    let script = Script::new();
    answer(&script, 200, "title");
    let heard = Arc::new(AtomicBool::new(false));
    let flag = heard.clone();
    let issuer = Issuer::with_delays(&[secs(3), Duration::ZERO]);
    let client = docs(
        &script,
        issuer
            .renewed()
            .on_renewed(move |_| flag.store(true, Ordering::SeqCst)),
    );

    assert!(timeout(secs(1), client.title().send()).await.is_err());
    sleep(secs(5)).await;
    assert!(!heard.load(Ordering::SeqCst), "nothing was published");

    success(client.title().await);
    assert_eq!(issuer.count(), 2);
    assert_eq!(authorizations(&script), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_renewal_error_find_reaches_the_typed_error_from_any_level() {
    // From the error of a generated call, from the cause below it and from the renewal
    // error itself; and below that, the error of the token endpoint is the renewal's source.
    let tokens_script = Script::new();
    tokens_script.answer(500).body("down");
    let tokens = Tokens::new().with_transport(tokens_script.clone());
    let script = Script::new();
    answer(&script, 200, "title");
    let auth = renewed(move || {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.issue().await?;
            AccessToken::expires_in(reply, secs(3600))
        }
    });

    let error = failure(docs(&script, auth).title().await);

    let found = RenewalError::find(&error).expect("found from the call error");
    assert!(matches!(found, RenewalError::Failed(_)));
    let token_call = found
        .source()
        .and_then(|cause| cause.downcast_ref::<concord::Error>())
        .expect("the cause below is the error of the token call");
    assert!(token_call.is_status(500));
    let below = error.source().expect("the call error has a source");
    assert!(matches!(
        RenewalError::find(below),
        Some(RenewalError::Failed(_))
    ));
    assert!(matches!(
        RenewalError::find(renewal_error(&error)),
        Some(RenewalError::Failed(_))
    ));

    // The token call's own error holds no renewal error; neither does a 401 of the API.
    assert!(RenewalError::find(token_call).is_none());
    let script = Script::new();
    answer(&script, 401, "denied");
    let refused = failure(docs(&script, Issuer::new().renewed()).title().await);
    assert!(RenewalError::find(&refused).is_none());
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_renewal_that_panics_fails_every_waiter_and_the_next_call_renews() {
    // No call is left hung, and the value stays usable.
    let script = Script::new();
    answer(&script, 200, "title");
    let count = Arc::new(AtomicUsize::new(0));
    let counted = count.clone();
    let auth = renewed(move || {
        let n = counted.fetch_add(1, Ordering::SeqCst) + 1;
        async move {
            sleep(secs(1)).await;
            if n == 1 {
                panic!("{MARK}-in-a-panic");
            }
            AccessToken::expires_in(format!("{MARK}-{n}"), secs(3600))
        }
    });
    let client = docs(&script, auth);
    let start = Instant::now();

    let (a, b, c) = tokio::join!(
        client.title().send(),
        client.title().send(),
        client.title().send()
    );
    for result in [a, b, c] {
        let error = failure(result);
        assert!(matches!(renewal_error(&error), RenewalError::Panicked));
        assert!(renewal_error(&error).source().is_none(), "no payload");
    }
    assert_eq!(start.elapsed(), secs(1));

    success(client.title().await);
    assert_eq!(count.load(Ordering::SeqCst), 2);
    assert_eq!(authorizations(&script), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_notification_or_a_header_that_panics_publishes_nothing() {
    // `on_renewed` panics once: the waiters get `Panicked`, the token is not kept, and
    // the next call renews.
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();
    let once = Arc::new(AtomicBool::new(true));
    let auth = issuer.renewed().on_renewed(move |_| {
        if once.swap(false, Ordering::SeqCst) {
            panic!("a notification of the author's");
        }
    });
    let client = docs(&script, auth);

    let error = failure(client.title().await);
    assert!(matches!(renewal_error(&error), RenewalError::Panicked));
    success(client.title().await);
    assert_eq!(issuer.count(), 2);
    assert_eq!(authorizations(&script), vec![bearer(2)]);

    // A header formatter that panics once.
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new();
    let once = Arc::new(AtomicBool::new(true));
    let auth = issuer
        .renewed()
        .header(HeaderName::from_static("x-api-key"), move |token| {
            if once.swap(false, Ordering::SeqCst) {
                panic!("a formatter of the author's");
            }
            HeaderValue::from_str(&token.expose())
        });
    let client = docs(&script, auth);

    let error = failure(client.title().await);
    assert!(matches!(renewal_error(&error), RenewalError::Panicked));
    prints_no_token(&printed(&error));
    success(client.title().await);
    assert_eq!(script.requests().len(), 1);
    assert_eq!(script.requests()[0].header("x-api-key"), Some("s3cr3t-2"));
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_seed_inside_the_margin_is_accepted_in_any_order_and_the_first_call_renews() {
    // A seed of 10 s with a margin of 20 s, the margin set before or after the seed.
    for margin_first in [true, false] {
        let script = Script::new();
        answer(&script, 200, "title");
        let issuer = Issuer::new();
        let seed = AccessToken::expires_in("s3cr3t-0", secs(10)).expect("a token of 10 s");
        let auth = if margin_first {
            issuer.renewed().early_by(secs(20)).with_token(seed)
        } else {
            issuer
                .renewed()
                .with_token(seed)
                .map(|auth| auth.early_by(secs(20)))
        };
        let auth = match auth {
            Ok(auth) => auth,
            Err(error) => panic!("margin first {margin_first}: the seed is refused: {error:?}"),
        };

        success(docs(&script, auth).title().await);

        assert_eq!(
            authorizations(&script),
            vec![bearer(1)],
            "margin first {margin_first}"
        );
        assert_eq!(issuer.count(), 1, "margin first {margin_first}");
    }
}

#[tokio::test(start_paused = true)]
async fn c11_3_with_token_refuses_only_a_token_that_has_already_expired() {
    // A seed that expired after it was made.
    let seed = AccessToken::expires_in("s3cr3t-0", secs(10)).expect("a token of 10 s");
    sleep(secs(10)).await;
    assert!(matches!(
        Issuer::new().renewed().with_token(seed).map(|_| ()),
        Err(RenewalError::Expired)
    ));

    // One second left, whatever the margin: accepted.
    let seed = AccessToken::expires_in("s3cr3t-0", secs(1)).expect("a token of 1 s");
    assert!(Issuer::new()
        .renewed()
        .early_by(secs(3600))
        .with_token(seed)
        .is_ok());
}

#[tokio::test(start_paused = true)]
async fn c11_3_a_renewed_token_that_lives_exactly_the_margin_is_refused() {
    // "does not outlast `early_by`": a lifetime equal to the margin is refused, one
    // second longer is accepted.
    let script = Script::new();
    answer(&script, 200, "title");
    let issuer = Issuer::new().lifetime(secs(60));
    let error = failure(
        docs(&script, issuer.renewed().early_by(secs(60)))
            .title()
            .await,
    );
    assert!(matches!(renewal_error(&error), RenewalError::Expired));
    assert_eq!(script.requests().len(), 0);

    let issuer = Issuer::new().lifetime(secs(61));
    success(
        docs(&script, issuer.renewed().early_by(secs(60)))
            .title()
            .await,
    );
    assert_eq!(authorizations(&script), vec![bearer(1)]);
}
