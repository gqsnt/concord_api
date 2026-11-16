//! C12.6 to C12.8: the rotating credential, `concord::auth::Rotating` (feature `rotating`).
//!
//! Written from the public surface and its rustdoc alone: `Rotating`, `RotationDriver`,
//! `CredentialStore`, `Stored`, `TokenPair`, `Exchange`, `RotationError` and `after_401`,
//! attached to a client of `concord::api!` over a `concord_testkit::Script`, on paused
//! `tokio` time. The store and the exchange are the test's own: each records what it was
//! asked, in order, with the number of resource requests sent at that moment.

use concord::auth::{
    after_401, CredentialStore, Exchange, RenewalError, Rotating, RotationDriver, RotationError,
    Stored, TokenPair,
};
use concord::http::{HeaderName, HeaderValue};
use concord::{Base, Retry};
use concord_testkit::Script;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tokio::task::JoinHandle;
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
    }
}

concord::api! {
    client Tokens {
        base "https://identity.example.test";
        POST exchange(refresh: concord::Secret) -> String {
            path "token";
            body concord::body::form { "grant_type" = "refresh_token"; "refresh_token" = refresh; }
            read concord::Text;
        }
    }
}

// What the tests share ----------------------------------------------------------------

/// The text that every token holds, so that a printed line can be searched for it.
const MARK: &str = "s3cr3t";

const HOUR: Duration = Duration::from_secs(3600);

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

fn base() -> Base {
    "https://docs.example.test/v2"
        .parse()
        .expect("the base parses")
}

fn access(n: usize) -> String {
    format!("{MARK}-a{n}")
}

fn refresh(n: usize) -> String {
    format!("{MARK}-r{n}")
}

fn bearer(n: usize) -> Option<String> {
    Some(format!("Bearer {}", access(n)))
}

/// What the store and the token endpoint were asked, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ev {
    /// A `load` that returned (with success or not).
    Load,
    /// A `save` that returned (with success or not), with the record it was given.
    Save(Rec),
    /// An exchange that was invoked, with the refresh token it was given.
    Exchange(String),
}

/// A record, as text.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Rec {
    Ready(String, String),
    Refreshing,
}

fn ready(n: usize) -> Ev {
    Ev::Save(Rec::Ready(access(n), refresh(n)))
}

fn mark() -> Ev {
    Ev::Save(Rec::Refreshing)
}

fn exchanged(n: usize) -> Ev {
    Ev::Exchange(refresh(n))
}

/// The three steps of the `n`-th renewal: the mark, the exchange of `r(n-1)`, the new pair.
fn renewal(n: usize) -> Vec<Ev> {
    vec![mark(), exchanged(n - 1), ready(n)]
}

fn rec(record: &Stored) -> Rec {
    match record {
        Stored::Ready(pair) => Rec::Ready(
            pair.access().expose().to_string(),
            pair.refresh().expose().to_string(),
        ),
        Stored::Refreshing => Rec::Refreshing,
    }
}

/// What one store operation does.
#[derive(Clone, Copy, Debug)]
enum Op {
    Ok,
    /// Fails, and storage is unchanged.
    Fail,
    /// Fails, and storage was replaced all the same: an outcome that the caller cannot know.
    FailAfterCommit,
    /// Waits, then succeeds.
    Delay(Duration),
    /// Never returns.
    Hang,
    /// Panics with a token in the payload.
    Panic,
}

/// What one exchange does. `n` is the number of the exchange, from 1.
#[derive(Clone, Copy, Debug)]
enum Issue {
    /// After `delay`, `(a<n>, r<n>)` with this lifetime (`None`: `TokenPair::new(…, None)`).
    Pair {
        delay: Duration,
        lifetime: Option<Duration>,
    },
    /// An error of the author's, with a token in its text.
    Fail,
    Panic,
    /// Returns the refresh token it consumed.
    Same,
    /// Returns an empty refresh token.
    Empty,
    /// `(this access text, r<n>)`, valid for an hour.
    Access(&'static str),
}

fn after(delay: Duration) -> Issue {
    Issue::Pair {
        delay,
        lifetime: Some(HOUR),
    }
}

/// An error of the author's whose text holds a token.
#[derive(Debug)]
struct Failure;

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "failed with {MARK}-in-a-cause")
    }
}

impl std::error::Error for Failure {}

struct Entry {
    ev: Ev,
    sent: usize,
    at: Instant,
}

/// The account: the record, the plans of the store and of the exchange, and the log.
struct World {
    script: Script,
    record: Mutex<Stored>,
    entries: Mutex<Vec<Entry>>,
    loads: Mutex<VecDeque<Op>>,
    saves: Mutex<VecDeque<Op>>,
    issues: Mutex<VecDeque<Issue>>,
    issued: AtomicUsize,
    /// Exchange futures alive.
    live: AtomicUsize,
    /// What `Debug` printed for each `Exchange`.
    printed: Mutex<Vec<String>>,
}

impl World {
    fn new(initial: Stored) -> Arc<World> {
        Arc::new(World {
            script: Script::new(),
            record: Mutex::new(initial),
            entries: Mutex::default(),
            loads: Mutex::default(),
            saves: Mutex::default(),
            issues: Mutex::default(),
            issued: AtomicUsize::new(0),
            live: AtomicUsize::new(0),
            printed: Mutex::default(),
        })
    }

    fn loads(self: &Arc<Self>, ops: &[Op]) -> Arc<Self> {
        self.loads.lock().unwrap().extend(ops.iter().copied());
        self.clone()
    }

    fn saves(self: &Arc<Self>, ops: &[Op]) -> Arc<Self> {
        self.saves.lock().unwrap().extend(ops.iter().copied());
        self.clone()
    }

    fn issues(self: &Arc<Self>, issues: &[Issue]) -> Arc<Self> {
        self.issues.lock().unwrap().extend(issues.iter().copied());
        self.clone()
    }

    fn log(&self, ev: Ev) {
        let sent = self.script.requests().len();
        self.entries.lock().unwrap().push(Entry {
            ev,
            sent,
            at: Instant::now(),
        });
    }

    fn events(&self) -> Vec<Ev> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.ev.clone())
            .collect()
    }

    /// The refresh tokens given to the exchange, in order.
    fn exchanges(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|ev| match ev {
                Ev::Exchange(token) => Some(token),
                _ => None,
            })
            .collect()
    }

    /// For each event: how many resource requests had been sent.
    fn sent_at_each(&self) -> Vec<usize> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .map(|entry| entry.sent)
            .collect()
    }

    fn at(&self, index: usize) -> Instant {
        self.entries.lock().unwrap()[index].at
    }

    fn live(&self) -> usize {
        self.live.load(Ordering::SeqCst)
    }

    fn stored(&self) -> Rec {
        rec(&self.record.lock().unwrap())
    }

    /// The `authorization` header of every request that reached the script.
    fn authorizations(&self) -> Vec<Option<String>> {
        self.script
            .requests()
            .iter()
            .map(|request| request.header("authorization").map(str::to_owned))
            .collect()
    }

    fn answer(&self, status: u16) {
        self.script.answer(status).body("body");
    }

    fn sent(&self) -> usize {
        self.script.requests().len()
    }
}

#[derive(Clone)]
struct Store(Arc<World>);

impl CredentialStore for Store {
    async fn load(&self) -> Result<Stored, RenewalError> {
        let op = self.0.loads.lock().unwrap().pop_front().unwrap_or(Op::Ok);
        match op {
            Op::Delay(delay) => sleep(delay).await,
            Op::Hang => std::future::pending::<()>().await,
            Op::Panic => panic!("{MARK}-in-a-load-panic"),
            _ => {}
        }
        self.0.log(Ev::Load);
        match op {
            Op::Fail | Op::FailAfterCommit => Err(RenewalError::failed(Failure)),
            _ => Ok(self.0.record.lock().unwrap().clone()),
        }
    }

    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        let op = self.0.saves.lock().unwrap().pop_front().unwrap_or(Op::Ok);
        let text = rec(&record);
        match op {
            Op::Delay(delay) => sleep(delay).await,
            Op::Hang => std::future::pending::<()>().await,
            Op::Panic => panic!("{MARK}-in-a-save-panic"),
            _ => {}
        }
        if !matches!(op, Op::Fail) {
            *self.0.record.lock().unwrap() = record;
        }
        self.0.log(Ev::Save(text));
        match op {
            Op::Fail | Op::FailAfterCommit => Err(RenewalError::failed(Failure)),
            _ => Ok(()),
        }
    }
}

/// Counts the exchange futures that are alive.
struct Live(Arc<World>);

impl Drop for Live {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The test's own exchange: records the refresh token it was given, then follows its plan.
fn exchange(
    world: Arc<World>,
    input: Exchange,
) -> impl Future<Output = Result<TokenPair, RenewalError>> + Send + 'static {
    let token = input.refresh().expose().to_string();
    world.printed.lock().unwrap().push(format!("{input:?}"));
    world.log(Ev::Exchange(token.clone()));
    let n = world.issued.fetch_add(1, Ordering::SeqCst) + 1;
    let issue = world
        .issues
        .lock()
        .unwrap()
        .pop_front()
        .unwrap_or(Issue::Pair {
            delay: Duration::ZERO,
            lifetime: Some(HOUR),
        });
    world.live.fetch_add(1, Ordering::SeqCst);
    let live = Live(world);
    async move {
        let _live = live;
        match issue {
            Issue::Pair { delay, lifetime } => {
                if !delay.is_zero() {
                    sleep(delay).await;
                }
                Ok(match lifetime {
                    Some(lifetime) => TokenPair::expires_in(access(n), refresh(n), lifetime),
                    None => TokenPair::new(access(n), refresh(n), None),
                })
            }
            Issue::Fail => Err(RenewalError::failed(Failure)),
            Issue::Panic => panic!("{MARK}-in-an-exchange-panic"),
            Issue::Same => Ok(TokenPair::expires_in(access(n), token, HOUR)),
            Issue::Empty => Ok(TokenPair::expires_in(access(n), "", HOUR)),
            Issue::Access(text) => Ok(TokenPair::expires_in(text, refresh(n), HOUR)),
        }
    }
}

/// A pair `(a0, r0)` valid for an hour of wall time.
fn fresh() -> Stored {
    Stored::Ready(TokenPair::new(
        access(0),
        refresh(0),
        SystemTime::now().checked_add(HOUR),
    ))
}

/// A pair `(a0, r0)` whose access cannot be used.
fn expired() -> Stored {
    Stored::Ready(TokenPair::new(access(0), refresh(0), None))
}

fn credential(world: &Arc<World>) -> (Rotating, RotationDriver) {
    let for_exchange = world.clone();
    Rotating::new(&base(), Store(world.clone()), move |input: Exchange| {
        exchange(for_exchange.clone(), input)
    })
}

fn spawn(driver: RotationDriver) -> JoinHandle<Result<(), RotationError>> {
    tokio::spawn(driver)
}

fn docs(world: &World, auth: Rotating) -> Docs {
    Docs::new()
        .with_middleware(auth)
        .with_transport(world.script.clone())
}

/// A client over the world's script whose driver is polled by a task of the test.
fn client(world: &Arc<World>) -> (Docs, JoinHandle<Result<(), RotationError>>) {
    let (auth, driver) = credential(world);
    (docs(world, auth), spawn(driver))
}

#[track_caller]
fn success<T>(result: Result<T, concord::Error>) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!(
            "expected a value, got an error {:?} (status {:?}): {:?}",
            error.kind(),
            error.status(),
            RotationError::find(&error)
        ),
    }
}

#[track_caller]
fn failure<T>(result: Result<T, concord::Error>) -> concord::Error {
    match result {
        Ok(_) => panic!("expected an error, got a value"),
        Err(error) => error,
    }
}

/// The rotation error that a call ended with.
#[track_caller]
fn rotation(result: Result<String, concord::Error>) -> RotationError {
    let error = failure(result);
    RotationError::find(&error)
        .cloned()
        .expect("the call error holds a RotationError")
}

#[track_caller]
fn retained_failure(error: &RenewalError) {
    match error {
        RenewalError::Failed(cause) => assert!(
            cause.downcast_ref::<Failure>().is_some(),
            "the author's error is retained"
        ),
        other => panic!("expected Failed, got {other:?}"),
    }
}

/// What an error prints: its line and its `Debug`, and the same for the rotation and the
/// renewal errors in it (not the author's own cause, which `source()` exposes on purpose).
fn printed(error: &concord::Error) -> String {
    let mut text = format!("{error}\n{error:?}");
    if let Some(rotation) = RotationError::find(error) {
        text.push_str(&format!("\n{rotation}\n{rotation:?}"));
        if let RotationError::Load(inner)
        | RotationError::Save(inner)
        | RotationError::Renewal(inner) = rotation
        {
            text.push_str(&format!("\n{inner}\n{inner:?}"));
        }
    }
    text
}

#[track_caller]
fn prints_no_token(text: &str) {
    assert!(!text.contains(MARK), "a printed text holds a token: {text}");
}

// C12.6 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_6_a_usable_pair_is_loaded_and_sent_without_an_exchange() {
    let world = World::new(fresh());
    world.answer(200);
    let (client, _driver) = client(&world);

    success(client.title().await);

    assert_eq!(world.events(), vec![Ev::Load]);
    assert_eq!(world.authorizations(), vec![bearer(0)]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_the_mark_precedes_the_exchange_and_the_pair_is_saved_before_it_is_sent() {
    let world = World::new(expired()).saves(&[Op::Delay(secs(2)), Op::Delay(secs(3))]);
    world.answer(200);
    let (client, _driver) = client(&world);
    let start = Instant::now();

    success(client.title().await);

    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected);
    assert_eq!(
        world.sent_at_each(),
        vec![0, 0, 0, 0],
        "nothing sent before the save"
    );
    // The exchange starts only once the mark's save returned (after its 2 s).
    assert_eq!(world.at(2), start + secs(2));
    let requests = world.script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].header("authorization"), bearer(1).as_deref());
    assert_eq!(
        requests[0].at(),
        start + secs(5),
        "sent when the save returned"
    );
    assert_eq!(world.stored(), Rec::Ready(access(1), refresh(1)));
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_mark_that_fails_to_save_stops_before_the_exchange() {
    let world = World::new(expired()).saves(&[Op::Fail]);
    let (client, _driver) = client(&world);

    match rotation(client.title().await) {
        RotationError::Save(inner) => retained_failure(&inner),
        other => panic!("expected Save, got {other:?}"),
    }

    assert_eq!(world.events(), vec![Ev::Load, mark()]);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_after_a_mark_that_was_not_saved_the_next_call_reloads_and_renews() {
    let world = World::new(expired()).saves(&[Op::Fail]);
    world.answer(200);
    let (client, _driver) = client(&world);

    let _ = failure(client.title().await);
    success(client.title().await);

    let mut expected = vec![Ev::Load, mark(), Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected);
    assert_eq!(world.authorizations(), vec![bearer(1)]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_mark_whose_save_is_unclear_is_reloaded_and_never_leads_to_the_exchange() {
    // The save reports an error, but the mark is in storage.
    let world = World::new(expired()).saves(&[Op::FailAfterCommit]);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Save(_)
    ));
    assert!(matches!(
        rotation(client.title().await),
        RotationError::NeedsReauthorization
    ));
    assert!(matches!(
        rotation(client.title().await),
        RotationError::NeedsReauthorization
    ));

    let events = world.events();
    assert_eq!(&events[..3], &[Ev::Load, mark(), Ev::Load], "a reload");
    assert_eq!(world.exchanges(), Vec::<String>::new(), "r0 is never sent");
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_last_save_that_fails_is_retried_as_a_save_only() {
    let world = World::new(expired()).saves(&[Op::Ok, Op::Fail]);
    world.answer(200);
    let (client, _driver) = client(&world);

    match rotation(client.title().await) {
        RotationError::Save(inner) => retained_failure(&inner),
        other => panic!("expected Save, got {other:?}"),
    }
    assert_eq!(
        world.sent(),
        0,
        "the new pair is not used before it is saved"
    );

    success(client.title().await);

    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    expected.extend([Ev::Load, ready(1)]);
    assert_eq!(world.events(), expected);
    assert_eq!(world.authorizations(), vec![bearer(1)]);
    assert_eq!(world.stored(), Rec::Ready(access(1), refresh(1)));
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_last_save_that_fails_twice_is_still_never_exchanged_again() {
    let world = World::new(expired()).saves(&[Op::Ok, Op::Fail, Op::Fail]);
    world.answer(200);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Save(_)
    ));
    assert!(matches!(
        rotation(client.title().await),
        RotationError::Save(_)
    ));
    success(client.title().await);

    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.authorizations(), vec![bearer(1)]);
    let saves_of_the_pair = world.events().iter().filter(|ev| **ev == ready(1)).count();
    assert_eq!(saves_of_the_pair, 3);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_last_save_whose_outcome_is_unclear_is_reloaded_and_never_exchanged_again() {
    let world = World::new(expired()).saves(&[Op::Ok, Op::FailAfterCommit]);
    world.answer(200);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Save(_)
    ));
    success(client.title().await);

    let events = world.events();
    assert_eq!(events[4], Ev::Load, "the next demand reloads");
    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.authorizations(), vec![bearer(1)]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_exchange_that_fails_after_the_mark_is_never_repeated() {
    let world = World::new(expired()).issues(&[Issue::Fail]);
    let (client, _driver) = client(&world);

    match rotation(client.title().await) {
        RotationError::Renewal(inner) => retained_failure(&inner),
        other => panic!("expected Renewal, got {other:?}"),
    }
    for _ in 0..2 {
        assert!(matches!(
            rotation(client.title().await),
            RotationError::NeedsReauthorization
        ));
    }

    assert_eq!(&world.events()[..3], &[Ev::Load, mark(), exchanged(0)]);
    assert_eq!(world.exchanges(), vec![refresh(0)], "r0 is sent once");
    assert_eq!(world.stored(), Rec::Refreshing);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_the_token_endpoint_receives_each_refresh_token_once_through_a_client() {
    let world = World::new(expired());
    let tokens_script = Script::new();
    tokens_script
        .answer(200)
        .body(format!("{} {}", access(1), refresh(1)));
    tokens_script
        .answer(200)
        .body(format!("{} {}", access(2), refresh(2)));
    let tokens = Tokens::new().with_transport(tokens_script.clone());
    let (auth, driver) = Rotating::new(&base(), Store(world.clone()), move |input: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.exchange(input.refresh()).await?;
            let (access, refresh) = reply.split_once(' ').expect("two tokens");
            Ok(TokenPair::expires_in(access, refresh, HOUR))
        }
    });
    let _driver = spawn(driver);
    let client = docs(&world, auth);
    world.answer(200);
    world.answer(200);
    world.answer(401);
    world.answer(200);

    success(client.title().await);
    success(client.title().await);
    let _ = failure(client.title().await);
    success(client.title().await);

    let bodies: Vec<_> = tokens_script
        .requests()
        .iter()
        .map(|request| request.body().map(<[u8]>::to_vec))
        .collect();
    assert_eq!(
        bodies,
        vec![
            Some(b"grant_type=refresh_token&refresh_token=s3cr3t-r0".to_vec()),
            Some(b"grant_type=refresh_token&refresh_token=s3cr3t-r1".to_vec()),
        ]
    );
    assert_eq!(
        world.authorizations(),
        vec![bearer(1), bearer(1), bearer(1), bearer(2)]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_token_endpoint_that_fails_is_asked_once() {
    let world = World::new(expired());
    let tokens_script = Script::new();
    tokens_script.answer(500).body("try again");
    tokens_script
        .answer(200)
        .body(format!("{} {}", access(1), refresh(1)));
    let tokens = Tokens::new().with_transport(tokens_script.clone());
    let (auth, driver) = Rotating::new(&base(), Store(world.clone()), move |input: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.exchange(input.refresh()).await?;
            let (access, refresh) = reply.split_once(' ').expect("two tokens");
            Ok(TokenPair::expires_in(access, refresh, HOUR))
        }
    });
    let _driver = spawn(driver);
    let client = docs(&world, auth);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Renewal(RenewalError::Failed(_))
    ));
    assert!(matches!(
        rotation(client.title().await),
        RotationError::NeedsReauthorization
    ));

    assert_eq!(tokens_script.requests().len(), 1);
    assert_eq!(tokens_script.unused(), 1);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_access_token_without_expiry_is_saved_then_the_call_fails_expired() {
    let world = World::new(expired()).issues(&[Issue::Pair {
        delay: Duration::ZERO,
        lifetime: None,
    }]);
    world.answer(200);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Renewal(RenewalError::Expired)
    ));
    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected, "the replacement is saved first");
    assert_eq!(world.sent(), 0);

    // The saved replacement, not the consumed token, is exchanged next.
    success(client.title().await);
    assert_eq!(world.exchanges(), vec![refresh(0), refresh(1)]);
    assert_eq!(world.authorizations(), vec![bearer(2)]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_expiry_that_cannot_be_represented_is_no_expiry_and_is_saved_first() {
    assert!(TokenPair::expires_in("a", "r", Duration::MAX)
        .expires_at()
        .is_none());

    let world = World::new(expired()).issues(&[Issue::Pair {
        delay: Duration::ZERO,
        lifetime: Some(Duration::MAX),
    }]);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Renewal(RenewalError::Expired)
    ));
    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_access_token_that_cannot_be_a_header_is_saved_then_the_call_fails() {
    let world = World::new(expired()).issues(&[Issue::Access("s3cr3t-a1\n")]);
    let (client, _driver) = client(&world);

    let error = failure(client.title().await);

    assert!(matches!(
        RotationError::find(&error),
        Some(RotationError::Renewal(RenewalError::InvalidHeader(_)))
    ));
    assert_eq!(
        world.events(),
        vec![
            Ev::Load,
            mark(),
            exchanged(0),
            Ev::Save(Rec::Ready("s3cr3t-a1\n".into(), refresh(1))),
        ]
    );
    prints_no_token(&printed(&error));
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_empty_refresh_token_is_never_sent() {
    let world = World::new(Stored::Ready(TokenPair::new(access(0), "", None)));
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::InvalidRefreshToken
    ));
    assert_eq!(world.exchanges(), Vec::<String>::new());
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_exchange_that_returns_the_token_it_consumed_is_refused() {
    let world = World::new(expired()).issues(&[Issue::Same]);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::InvalidRefreshToken
    ));
    let _ = failure(client.title().await);

    assert_eq!(world.exchanges(), vec![refresh(0)], "r0 is sent once");
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_exchange_that_returns_an_empty_refresh_token_is_refused() {
    let world = World::new(expired()).issues(&[Issue::Empty]);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::InvalidRefreshToken
    ));
    let _ = failure(client.title().await);

    assert_eq!(world.exchanges(), vec![refresh(0)], "r0 is sent once");
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_mark_found_at_load_needs_reauthorization() {
    let world = World::new(Stored::Refreshing);
    let (client, _driver) = client(&world);

    assert!(matches!(
        rotation(client.title().await),
        RotationError::NeedsReauthorization
    ));
    assert_eq!(world.events(), vec![Ev::Load]);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_load_that_fails_keeps_its_cause_and_the_next_call_loads_again() {
    let world = World::new(fresh()).loads(&[Op::Fail]);
    world.answer(200);
    let (client, _driver) = client(&world);

    match rotation(client.title().await) {
        RotationError::Load(inner) => retained_failure(&inner),
        other => panic!("expected Load, got {other:?}"),
    }
    success(client.title().await);

    assert_eq!(world.events(), vec![Ev::Load, Ev::Load]);
    assert_eq!(world.authorizations(), vec![bearer(0)]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_relative_lifetime_counts_the_save_and_ends_exactly() {
    // `expires_in(60 s)` at the exchange; the save takes 10 s: 50 s are left after it.
    let world = World::new(expired())
        .saves(&[Op::Ok, Op::Delay(secs(10))])
        .issues(&[Issue::Pair {
            delay: Duration::ZERO,
            lifetime: Some(secs(60)),
        }]);
    for _ in 0..3 {
        world.answer(200);
    }
    let (client, _driver) = client(&world);
    let start = Instant::now();

    success(client.title().await);
    assert_eq!(Instant::now(), start + secs(10));

    sleep(secs(49)).await; // 1 s left
    success(client.title().await);
    assert_eq!(world.exchanges(), vec![refresh(0)], "1 s left: no renewal");

    sleep(secs(1)).await; // nothing left
    success(client.title().await);
    assert_eq!(world.exchanges(), vec![refresh(0), refresh(1)]);
    assert_eq!(
        world.authorizations(),
        vec![bearer(1), bearer(1), bearer(2)]
    );
}

// C12.7 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_7_a_call_that_waits_on_an_unpolled_driver_fails_not_polled_after_its_timeout() {
    let world = World::new(expired());
    let (auth, _driver) = credential(&world);
    let client = docs(&world, auth.renewal_timeout(secs(5)));
    let start = Instant::now();

    assert!(matches!(
        rotation(client.title().await),
        RotationError::NotPolled
    ));

    assert_eq!(Instant::now(), start + secs(5));
    assert_eq!(world.exchanges(), Vec::<String>::new());
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_the_default_wait_is_thirty_seconds() {
    let world = World::new(expired());
    let (auth, _driver) = credential(&world);
    let client = docs(&world, auth);
    let start = Instant::now();

    assert!(matches!(
        rotation(client.title().await),
        RotationError::NotPolled
    ));
    assert_eq!(Instant::now(), start + secs(30));
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_zero_wait_is_refused_before_the_store() {
    let world = World::new(expired());
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let client = docs(&world, auth.renewal_timeout(Duration::ZERO));

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Renewal(RenewalError::InvalidPolicy)
    ));
    assert_eq!(world.events(), Vec::<Ev>::new());
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_wait_that_ends_does_not_cancel_the_exchange() {
    let world = World::new(expired()).issues(&[after(secs(60))]);
    world.answer(200);
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let client = docs(&world, auth.renewal_timeout(secs(5)));
    let start = Instant::now();

    let renewal_error = rotation(client.title().await);
    assert!(
        matches!(
            renewal_error,
            RotationError::Renewal(RenewalError::TimedOut(_))
        ),
        "{renewal_error:?}"
    );
    assert_eq!(Instant::now(), start + secs(5));
    assert_eq!(world.live(), 1, "the exchange goes on");

    sleep(secs(56)).await;
    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected, "finished and saved");

    success(client.title().await);
    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.authorizations(), vec![bearer(1)]);
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_renewal_is_finished_and_saved_when_the_call_is_dropped_at_any_step() {
    // Each step takes 2 s: the mark's save (0 to 2 after the load), the exchange (2 to 4),
    // the pair's save (4 to 6). The call is dropped in the middle of each.
    for dropped_at in [1, 3, 5] {
        let world = World::new(expired())
            .saves(&[Op::Delay(secs(2)), Op::Delay(secs(2))])
            .issues(&[after(secs(2))]);
        world.answer(200);
        let (client, _driver) = client(&world);

        let dropped = timeout(secs(dropped_at), client.title()).await;
        assert!(dropped.is_err(), "{dropped_at}: the call was dropped");
        sleep(secs(20)).await;

        let mut expected = vec![Ev::Load];
        expected.extend(renewal(1));
        assert_eq!(world.events(), expected, "dropped at {dropped_at} s");
        assert_eq!(world.sent(), 0, "{dropped_at}: nothing sent");

        success(client.title().await);
        assert_eq!(world.exchanges(), vec![refresh(0)], "{dropped_at}");
        assert_eq!(world.authorizations(), vec![bearer(1)], "{dropped_at}");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_dropped_driver_stops_every_clone_at_once() {
    let world = World::new(fresh());
    let (auth, driver) = credential(&world);
    let other = docs(&world, auth.clone());
    let client = docs(&world, auth);
    drop(driver);
    let start = Instant::now();

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Stopped
    ));
    assert!(matches!(
        rotation(other.title().await),
        RotationError::Stopped
    ));

    assert_eq!(Instant::now(), start, "no wait");
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_driver_dropped_during_the_exchange_wakes_the_call_with_stopped() {
    let world = World::new(expired()).issues(&[after(secs(60))]);
    let (auth, driver) = credential(&world);
    tokio::spawn(async move {
        tokio::select! {
            _ = driver => {}
            _ = sleep(secs(5)) => {}
        }
    });
    let client = docs(&world, auth);
    let start = Instant::now();

    assert!(matches!(
        rotation(client.title().await),
        RotationError::Stopped
    ));
    assert_eq!(Instant::now(), start + secs(5));
    assert_eq!(
        world.live(),
        0,
        "the exchange was cancelled with the driver"
    );

    sleep(secs(60)).await;
    assert!(matches!(
        rotation(client.title().await),
        RotationError::Stopped
    ));
    assert_eq!(world.events(), vec![Ev::Load, mark(), exchanged(0)]);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_two_calls_at_once_share_one_renewal() {
    let world = World::new(expired())
        .saves(&[Op::Delay(secs(1)), Op::Delay(secs(1))])
        .issues(&[after(secs(2))]);
    for _ in 0..3 {
        world.answer(200);
    }
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let first = docs(&world, auth.clone());
    let second = docs(&world, auth.clone());
    let third = docs(&world, auth);

    let (a, b, c) = tokio::join!(first.title(), second.title(), third.title());
    success(a);
    success(b);
    success(c);

    assert_eq!(world.exchanges(), vec![refresh(0)]);
    let saves: Vec<_> = world
        .events()
        .into_iter()
        .filter(|ev| matches!(ev, Ev::Save(_)))
        .collect();
    assert_eq!(saves, vec![mark(), ready(1)]);
    assert_eq!(
        world.authorizations(),
        vec![bearer(1), bearer(1), bearer(1)]
    );
}

#[tokio::test(start_paused = true)]
async fn c12_7_the_driver_drains_started_work_then_returns_when_the_owners_are_gone() {
    let world = World::new(expired()).issues(&[after(secs(10))]);
    let (client, driver) = client(&world);
    let start = Instant::now();

    assert!(timeout(secs(1), client.title()).await.is_err());
    drop(client);

    let returned = driver.await.expect("the driver does not panic");
    assert!(returned.is_ok(), "{returned:?}");
    assert_eq!(Instant::now(), start + secs(10));
    let mut expected = vec![Ev::Load];
    expected.extend(renewal(1));
    assert_eq!(world.events(), expected);
}

#[tokio::test(start_paused = true)]
async fn c12_7_the_driver_returns_at_once_when_there_is_no_work_and_no_owner() {
    let world = World::new(fresh());
    let (auth, driver) = credential(&world);
    let clone = auth.clone();
    let task = spawn(driver);
    drop(auth);
    sleep(secs(1)).await;
    assert!(!task.is_finished(), "a clone still owns the account");
    drop(clone);

    let returned = timeout(secs(1), task).await.expect("the driver returns");
    assert!(returned.expect("no panic").is_ok());
    assert_eq!(world.events(), Vec::<Ev>::new());
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_store_panic_stops_the_account_and_prints_nothing() {
    let world = World::new(expired()).saves(&[Op::Panic]);
    let (client, driver) = client(&world);

    let first = failure(client.title().await);
    prints_no_token(&printed(&first));
    let second = failure(client.title().await);
    prints_no_token(&printed(&second));

    assert!(matches!(
        RotationError::find(&second),
        Some(RotationError::Stopped)
    ));
    let returned = timeout(secs(1), driver)
        .await
        .expect("the driver stops at once");
    assert!(returned.is_ok(), "the panic does not escape the driver");
    assert_eq!(world.exchanges(), Vec::<String>::new());
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_hung_save_bounds_the_call_and_the_second_caller_but_not_the_driver() {
    // The pair's save never returns: each call still ends at its own deadline, and the
    // driver, polled, cannot drain.
    let world = World::new(expired()).saves(&[Op::Ok, Op::Hang]);
    let (auth, driver) = credential(&world);
    let task = spawn(driver);
    let client = docs(&world, auth.renewal_timeout(secs(5)));
    let start = Instant::now();

    let first = rotation(client.title().await);
    assert!(
        matches!(first, RotationError::Renewal(RenewalError::TimedOut(_))),
        "{first:?}"
    );
    assert_eq!(Instant::now(), start + secs(5));
    let second = rotation(client.title().await);
    assert!(
        matches!(second, RotationError::Renewal(RenewalError::TimedOut(_))),
        "{second:?}"
    );
    assert_eq!(Instant::now(), start + secs(10));

    drop(client);
    sleep(secs(60)).await;
    assert!(!task.is_finished(), "a hung store prevents draining");
    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.sent(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_7_an_exchange_that_panics_fails_the_call_and_is_not_repeated() {
    let world = World::new(expired()).issues(&[Issue::Panic]);
    let (client, _driver) = client(&world);

    let error = failure(client.title().await);
    assert!(matches!(
        RotationError::find(&error),
        Some(RotationError::Renewal(RenewalError::Panicked))
    ));
    prints_no_token(&printed(&error));
    assert!(matches!(
        rotation(client.title().await),
        RotationError::NeedsReauthorization
    ));

    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.sent(), 0);
}

#[test]
fn c12_7_the_driver_is_send_and_unpin_and_the_credential_is_clone() {
    fn send_unpin<T: Send + Unpin + Future<Output = Result<(), RotationError>>>() {}
    fn clone_send_sync<T: Clone + Send + Sync>() {}
    send_unpin::<RotationDriver>();
    clone_send_sync::<Rotating>();
}

// C12.8 ------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn c12_8_the_token_goes_as_one_sensitive_bearer_header() {
    let world = World::new(expired());
    world.answer(200);
    let (client, _driver) = client(&world);

    success(client.title().await);

    let requests = world.script.requests();
    let values: Vec<_> = requests[0]
        .headers()
        .get_all("authorization")
        .iter()
        .collect();
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].as_bytes(), b"Bearer s3cr3t-a1");
    assert!(values[0].is_sensitive());
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_header_of_the_authors_is_sensitive_and_replaces_bearer() {
    let world = World::new(fresh());
    world.answer(200);
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let auth = auth.header(HeaderName::from_static("x-token"), |token| {
        HeaderValue::from_str(&format!("Token {}", &*token.expose()))
    });

    success(docs(&world, auth).title().await);

    let requests = world.script.requests();
    assert_eq!(requests[0].header("x-token"), Some("Token s3cr3t-a0"));
    assert!(requests[0]
        .headers()
        .get("x-token")
        .is_some_and(HeaderValue::is_sensitive));
    assert_eq!(requests[0].header("authorization"), None);
}

#[tokio::test(start_paused = true)]
async fn c12_8_another_origin_is_refused_before_the_store_is_used() {
    for other in [
        "https://other.example.test/v2",
        "https://api.docs.example.test/v2",
        "http://docs.example.test/v2",
        "https://docs.example.test:8443/v2",
    ] {
        let world = World::new(expired());
        world.answer(200);
        let (client, _driver) = client(&world);

        let found = rotation(client.with_base(other).title().await);

        assert!(
            matches!(found, RotationError::Renewal(RenewalError::WrongOrigin)),
            "{other}: {found:?}"
        );
        assert_eq!(world.events(), Vec::<Ev>::new(), "{other}: store untouched");
        assert_eq!(world.sent(), 0, "{other}: nothing sent");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_the_same_origin_written_otherwise_gets_the_token() {
    for same in [
        "https://docs.example.test/other",
        "https://docs.example.test:443/v2",
        "https://DOCS.example.test/v2",
    ] {
        let world = World::new(fresh());
        world.answer(200);
        let (client, _driver) = client(&world);

        success(client.with_base(same).title().await);

        assert_eq!(world.authorizations(), vec![bearer(0)], "{same}");
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_debug_prints_no_token() {
    let pair = TokenPair::new(access(0), refresh(0), None);
    prints_no_token(&format!("{pair:?}"));
    prints_no_token(&format!("{:?}", Stored::Ready(pair)));
    prints_no_token(&format!("{:?}", Stored::Refreshing));
    for error in [
        RotationError::Load(RenewalError::failed(Failure)),
        RotationError::Save(RenewalError::failed(Failure)),
        RotationError::Renewal(RenewalError::failed(Failure)),
    ] {
        prints_no_token(&format!("{error}\n{error:?}"));
    }

    let world = World::new(expired());
    world.answer(200);
    let (auth, driver) = credential(&world);
    prints_no_token(&format!("{auth:?}\n{driver:?}"));
    let _driver = spawn(driver);
    let client = docs(&world, auth.clone());
    success(client.title().await);
    prints_no_token(&format!("{auth:?}\n{client:?}"));
    let printed = world.printed.lock().unwrap().clone();
    assert_eq!(printed.len(), 1);
    prints_no_token(&printed[0]);
}

#[tokio::test(start_paused = true)]
async fn c12_8_an_exchange_error_is_retained_and_not_printed() {
    let world = World::new(expired()).issues(&[Issue::Fail]);
    let (client, _driver) = client(&world);

    let error = failure(client.title().await);

    prints_no_token(&printed(&error));
    let rotation = RotationError::find(&error).expect("a RotationError");
    // `source()` deliberately exposes the retained cause: the author's error is in the chain.
    let mut cause = std::error::Error::source(rotation);
    let mut found = false;
    while let Some(next) = cause {
        found |= next.downcast_ref::<Failure>().is_some();
        cause = next.source();
    }
    assert!(found, "source() leads to the author's error");
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_401_is_returned_unchanged_sent_once_and_the_next_call_renews() {
    let world = World::new(fresh());
    world
        .script
        .answer(401)
        .header("www-authenticate", "Bearer error=\"invalid_token\"")
        .body("denied");
    world.answer(200);
    let (client, _driver) = client(&world);

    let error = failure(client.title().await);
    assert!(error.is_status(401));
    assert_eq!(error.tries(), 1);
    assert_eq!(error.body(), Some(&b"denied"[..]));
    assert_eq!(world.sent(), 1, "the middleware sends nothing again");
    assert_eq!(world.exchanges(), Vec::<String>::new());

    success(client.title().await);
    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.authorizations(), vec![bearer(0), bearer(1)]);
}

#[tokio::test(start_paused = true)]
async fn c12_8_after_401_asks_the_engine_for_one_attempt_with_a_new_token() {
    let world = World::new(fresh());
    world.answer(401);
    world.answer(401);
    world.answer(200);
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let client = docs(&world, auth).with_retry(Retry::none().tries(3).when(after_401));

    // The second attempt gets a 401 too: only a first-attempt 401 asks for another.
    let error = failure(client.title().await);

    assert!(error.is_status(401));
    assert_eq!(error.tries(), 2);
    assert_eq!(world.authorizations(), vec![bearer(0), bearer(1)]);
    assert_eq!(world.exchanges(), vec![refresh(0)]);
    assert_eq!(world.script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_8_after_401_does_not_resend_an_unmarked_post() {
    let world = World::new(fresh());
    world.answer(401);
    world.answer(200);
    let (auth, driver) = credential(&world);
    let _driver = spawn(driver);
    let client = docs(&world, auth).with_retry(Retry::standard().when(after_401));

    let error = failure(client.note(String::from("text")).await);

    assert!(error.is_status(401));
    assert_eq!(world.sent(), 1);
    assert_eq!(world.script.unused(), 1);
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_401_after_a_renewal_rejects_the_new_token() {
    let world = World::new(expired());
    world.answer(401);
    world.answer(200);
    let (client, _driver) = client(&world);

    let _ = failure(client.title().await);
    success(client.title().await);

    assert_eq!(world.exchanges(), vec![refresh(0), refresh(1)]);
    assert_eq!(world.authorizations(), vec![bearer(1), bearer(2)]);
    assert_eq!(world.stored(), Rec::Ready(access(2), refresh(2)));
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_late_401_rejects_only_its_own_generation() {
    let world = World::new(fresh());
    world.script.answer(401).after(secs(10)).body("late"); // A, with a0
    world.answer(401); // B, with a0
    world.answer(200); // C, renews to a1
    world.answer(200); // D, after A's late 401
    let (client, _driver) = client(&world);

    let slow = client.clone();
    let a = tokio::spawn(async move { slow.title().await.is_err() });
    sleep(secs(1)).await;
    let _ = failure(client.title().await); // B
    sleep(secs(1)).await;
    success(client.title().await); // C
    assert!(a.await.expect("A runs"), "A got its 401");
    success(client.title().await); // D

    assert_eq!(
        world.exchanges(),
        vec![refresh(0)],
        "a1 is not rejected by A"
    );
    assert_eq!(
        world.authorizations(),
        vec![bearer(0), bearer(0), bearer(1), bearer(1)]
    );
}
