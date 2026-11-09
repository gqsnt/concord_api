//! Shared test support: fixtures, a client pointed at the loopback server, an adjustable token
//! clock, and a stateful fake of the documented PayPal rules this slice relies on (synthetic: it
//! implements the cited rules, it is not a recording of the service).

// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::future::Future;
use std::num::NonZeroUsize;
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_paypal::concord::reqwest;
use concord_sdk_paypal::{Builder, Credentials, PayPal};
use serde_json::Value;

/// Synthetic app credentials; no test may find the secret in formatted output.
pub const CLIENT_ID: &str = "synthetic-client-id";
pub const CLIENT_SECRET: &str = "synthetic-client-secret";
/// `Basic` + standard Base64 of `CLIENT_ID:CLIENT_SECRET`, computed outside the SDK
/// (`printf 'synthetic-client-id:synthetic-client-secret' | base64`).
pub const BASIC: &str = "Basic c3ludGhldGljLWNsaWVudC1pZDpzeW50aGV0aWMtY2xpZW50LXNlY3JldA==";

pub const ORDER: &str = "SYNTH0RDER1A2B3C";
pub const CAPTURE: &str = "SYNTHCAPTURE0001X";
pub const TOKEN_PATH: &str = "/v1/oauth2/token";
pub const ORDERS_PATH: &str = "/v2/checkout/orders";
pub const ORDER_PATH: &str = "/v2/checkout/orders/SYNTH0RDER1A2B3C";
pub const CAPTURE_ORDER_PATH: &str = "/v2/checkout/orders/SYNTH0RDER1A2B3C/capture";
pub const CAPTURE_PATH: &str = "/v2/payments/captures/SYNTHCAPTURE0001X";

/// Reads `fixtures/<name>`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// `fixtures/<name>` parsed as JSON, for expectations.
pub fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&fixture(name)).unwrap()
}

/// A reply with a fixture body labelled `application/json` (as PayPal labels its JSON).
pub fn json_fixture(status: u16, name: &str) -> Reply {
    Reply::new(status)
        .header("content-type", "application/json")
        .body(fixture(name))
}

/// The bearer value of a token fixture, read from the fixture.
pub fn bearer_of(token_fixture: &str) -> String {
    let token = fixture_json(token_fixture);
    format!("Bearer {}", token["access_token"].as_str().unwrap())
}

/// A transport like the SDK's `http_client()`, with a chosen request timeout.
pub fn http(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .unwrap()
}

pub fn credentials() -> Credentials {
    Credentials::new(CLIENT_ID, CLIENT_SECRET).unwrap()
}

/// The longest backoff of [`fast`].
pub const FAST_MAX_DELAY: Duration = Duration::from_millis(20);

/// Bounds with `attempts` and a 10 ms backoff, so tests do not wait for the defaults.
pub fn fast(attempts: usize) -> concord_sdk_paypal::concord_retry::Policy {
    concord_sdk_paypal::concord_retry::Policy {
        initial_delay: Duration::from_millis(10),
        max_delay: FAST_MAX_DELAY,
        ..concord_sdk_paypal::concord_retry::Policy::new(NonZeroUsize::new(attempts).unwrap())
    }
}

// ---------------------------------------------------------------------------------------------
// Timeouts on a paused clock
//
// A test that makes a write time out gives the client a short request timeout and has the server
// hold its answer. In real time that timeout also bounds everything else on the same client: the
// token request made before the write, the delivery of the write itself, and later calls. On a
// loaded machine any of them can run out first, and the test then fails with the wrong error.
// Such tests run on Tokio's paused clock (`#[tokio::test(start_paused = true)]`) and drive their
// calls with [`drive`] and [`time_out`]: the runtime is kept busy, so virtual time never jumps
// ahead by itself while loopback I/O is in flight, and it moves only once the server has received
// the attempt that is meant to time out.

/// Real time a driven call may take before the test reports it as stuck (twice the 5-second
/// client timeout the other tests allow a loopback exchange).
const STUCK: Duration = Duration::from_secs(10);

/// Timer deadlines are rounded up to the next millisecond.
const TICK: Duration = Duration::from_millis(1);

/// Polls `call` until it completes (`Some`) or `until()` holds (`None`), yielding between polls:
/// loopback I/O and the server's tasks progress while virtual time stands still.
async fn poll_until<F: Future>(
    mut call: Pin<&mut F>,
    until: impl Fn() -> bool,
) -> Option<F::Output> {
    let started = Instant::now();
    loop {
        if let Poll::Ready(output) = poll_once(call.as_mut()).await {
            return Some(output);
        }
        if until() {
            return None;
        }
        assert!(
            started.elapsed() < STUCK,
            "a driven call did not finish within {STUCK:?} of real time (an attempt the server \
             holds that the test did not expect to time out?)"
        );
        tokio::task::yield_now().await;
    }
}

async fn poll_once<F: Future>(mut call: Pin<&mut F>) -> Poll<F::Output> {
    std::future::poll_fn(|cx| Poll::Ready(call.as_mut().poll(cx))).await
}

/// Runs `call` to completion while virtual time stands still: no client timeout can run out.
pub async fn drive<F: Future>(call: F) -> F::Output {
    let mut call = pin!(call);
    poll_until(call.as_mut(), || false)
        .await
        .expect("only completion ends an unconditional drive")
}

/// Runs a write whose first `late` attempts the server answers after the client's `timeout`.
/// Each time the server has received the next late attempt to `path`, virtual time moves past
/// `timeout`, so that attempt times out after it reached the server, never before; if the session
/// then backs off before repeating the write, time moves past [`FAST_MAX_DELAY`] too. Everything
/// else (the token request, an attempt answered at once, the answers) completes while time stands
/// still. A call that ends before the server received a late attempt is returned as it ended.
pub async fn time_out<F: Future>(
    server: &TestServer,
    path: &str,
    timeout: Duration,
    late: usize,
    call: F,
) -> F::Output {
    let mut call = pin!(call);
    for attempt in 1..=late {
        let received = || {
            server
                .requests()
                .iter()
                .filter(|r| r.path() == path)
                .count()
                >= attempt
        };
        if let Some(ended) = poll_until(call.as_mut(), received).await {
            return ended;
        }
        tokio::time::advance(timeout + TICK).await;
        // The next poll observes the timeout: the call ends, or the session backs off.
        if let Poll::Ready(output) = poll_once(call.as_mut()).await {
            return output;
        }
        tokio::time::advance(FAST_MAX_DELAY + TICK).await;
    }
    drive(call).await
}

/// A builder pointed at the server with fast bounds (3 read attempts, 2 write attempts) and a
/// 5-second request timeout.
pub fn builder(server: &TestServer) -> Builder {
    builder_with(server, Duration::from_secs(5))
}

pub fn builder_with(server: &TestServer, timeout: Duration) -> Builder {
    PayPal::builder(http(timeout), credentials())
        .base_url(server.url())
        .reads(fast(3))
        .writes(fast(2))
}

pub fn api(server: &TestServer) -> PayPal {
    builder(server).build().unwrap()
}

/// A server that issues `token.json` and answers every other request with `answer`.
pub async fn serve(answer: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> TestServer {
    TestServer::start(move |r: &Recorded| {
        if r.path() == TOKEN_PATH {
            json_fixture(200, "token.json")
        } else {
            answer(r)
        }
    })
    .await
}

/// A monotonic clock the test moves forward.
#[derive(Clone)]
pub struct TestClock {
    start: Instant,
    offset_ms: Arc<AtomicU64>,
}

impl TestClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            offset_ms: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn advance(&self, by: Duration) {
        self.offset_ms
            .fetch_add(by.as_millis() as u64, Ordering::SeqCst);
    }

    pub fn reader(&self) -> impl Fn() -> Instant + Send + Sync + 'static {
        let (start, offset) = (self.start, self.offset_ms.clone());
        move || start + Duration::from_millis(offset.load(Ordering::SeqCst))
    }
}

/// Every request's method and target, in arrival order.
pub fn targets(server: &TestServer) -> Vec<String> {
    server
        .requests()
        .iter()
        .map(|r| format!("{} {}", r.method, r.path()))
        .collect()
}

pub fn count(server: &TestServer, method: &str, path: &str) -> usize {
    server
        .requests()
        .iter()
        .filter(|r| r.method == method && r.path() == path)
        .count()
}

/// How the fake answers one capture request.
#[derive(Clone, Copy, Debug)]
pub enum CapturePlan {
    /// Apply the documented rules and answer at once.
    Answer,
    /// Apply the rules, then hold the answer this long (a capture performed, answered late).
    AnswerAfter(Duration),
    /// Answer `500` without capturing.
    ServerError,
}

#[derive(Default)]
struct State {
    status: &'static str,
    issued: Vec<String>,
    token_fixtures: VecDeque<&'static str>,
    capture_plan: VecDeque<CapturePlan>,
    captured_with: Option<Option<String>>,
    captures_performed: usize,
    token_delay: Option<Duration>,
}

/// A stateful fake of the slice's documented rules:
///
/// - the token endpoint requires the app's `Basic` credentials and the form
///   `grant_type=client_credentials`, and issues the next token fixture (`token.json` unless
///   [`FakePayPal::next_tokens`] queued others);
/// - API calls require `Authorization: Bearer` with an issued token, else `401`
///   `AUTHENTICATION_FAILURE`;
/// - Create order answers `201` `CREATED`; the buyer's approval (outside the SDK) is
///   [`FakePayPal::approve`];
/// - Capture: `422 ORDER_NOT_APPROVED` before approval; once approved, the first capture
///   completes the order (`201`); a later capture with the same `PayPal-Request-Id` answers `200`
///   with the stored result, any other later capture `422 ORDER_ALREADY_CAPTURED`;
/// - Show captured payment details answers the capture once it exists, `404` before.
#[derive(Clone, Default)]
pub struct FakePayPal {
    state: Arc<Mutex<State>>,
    pub token_requests: Arc<AtomicUsize>,
}

impl FakePayPal {
    pub fn new() -> Self {
        let fake = Self::default();
        fake.state.lock().unwrap().status = "CREATED";
        fake
    }

    pub async fn start(&self) -> TestServer {
        let fake = self.clone();
        TestServer::start(move |r: &Recorded| fake.serve(r)).await
    }

    /// Queues the token fixtures to issue next, in order.
    pub fn next_tokens(&self, fixtures: &[&'static str]) {
        self.state
            .lock()
            .unwrap()
            .token_fixtures
            .extend(fixtures.iter().copied());
    }

    /// Holds every token answer this long (so concurrent callers overlap the acquisition).
    pub fn delay_tokens(&self, delay: Duration) {
        self.state.lock().unwrap().token_delay = Some(delay);
    }

    /// The buyer approves the order at the approval URL (outside the SDK).
    pub fn approve(&self) {
        self.state.lock().unwrap().status = "APPROVED";
    }

    pub fn plan_captures(&self, plan: &[CapturePlan]) {
        self.state
            .lock()
            .unwrap()
            .capture_plan
            .extend(plan.iter().copied());
    }

    pub fn captures_performed(&self) -> usize {
        self.state.lock().unwrap().captures_performed
    }

    pub fn status(&self) -> &'static str {
        self.state.lock().unwrap().status
    }

    /// Answers one request by the rules above.
    pub fn serve(&self, r: &Recorded) -> Reply {
        if r.path() == TOKEN_PATH {
            return self.token(r);
        }
        let mut state = self.state.lock().unwrap();
        let authorized = r
            .header("authorization")
            .is_some_and(|value| state.issued.iter().any(|t| value == format!("Bearer {t}")));
        if !authorized {
            return json_fixture(401, "error-authentication-failure.json");
        }
        match (r.method.as_str(), r.path()) {
            ("POST", ORDERS_PATH) => json_fixture(201, "order-created.json"),
            ("GET", ORDER_PATH) => match state.status {
                "CREATED" => json_fixture(200, "order-created.json"),
                "APPROVED" => json_fixture(200, "order-approved.json"),
                _ => json_fixture(200, "order-captured.json"),
            },
            ("POST", CAPTURE_ORDER_PATH) => {
                let plan = state
                    .capture_plan
                    .pop_front()
                    .unwrap_or(CapturePlan::Answer);
                if let CapturePlan::ServerError = plan {
                    return json_fixture(500, "error-internal-server-error.json");
                }
                let request_id = r.header("paypal-request-id").map(str::to_owned);
                let reply = match state.status {
                    "CREATED" => json_fixture(422, "error-order-not-approved.json"),
                    "APPROVED" => {
                        state.status = "COMPLETED";
                        state.captures_performed += 1;
                        state.captured_with = Some(request_id);
                        json_fixture(201, "order-captured.json")
                    }
                    _ => match (&state.captured_with, request_id) {
                        (Some(Some(stored)), Some(id)) if *stored == id => {
                            json_fixture(200, "order-captured.json")
                        }
                        _ => json_fixture(422, "error-order-already-captured.json"),
                    },
                };
                match plan {
                    CapturePlan::AnswerAfter(delay) => reply.delay(delay),
                    _ => reply,
                }
            }
            ("GET", CAPTURE_PATH) if state.status == "COMPLETED" => {
                json_fixture(200, "capture.json")
            }
            _ => json_fixture(404, "error-resource-not-found.json"),
        }
    }

    fn token(&self, r: &Recorded) -> Reply {
        self.token_requests.fetch_add(1, Ordering::SeqCst);
        let form = r.form_pairs();
        if r.method != "POST"
            || r.header("authorization") != Some(BASIC)
            || form != [("grant_type".to_owned(), "client_credentials".to_owned())]
        {
            return json_fixture(401, "token-invalid-client.json");
        }
        let mut state = self.state.lock().unwrap();
        let name = state.token_fixtures.pop_front().unwrap_or("token.json");
        let token = fixture_json(name)["access_token"]
            .as_str()
            .unwrap()
            .to_owned();
        state.issued.push(token);
        let reply = json_fixture(200, name);
        match state.token_delay {
            Some(delay) => reply.delay(delay),
            None => reply,
        }
    }
}
