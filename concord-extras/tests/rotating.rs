//! Contracts C12.6–C12.8 for rotating refresh tokens.
#![cfg(feature = "rotating")]
// Assertions, expected panics, and indexing also appear in scripted test helpers.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

use concord::auth::{
    after_401, CredentialStore, Exchange, RenewalError, Rotating, RotationDriver, RotationError,
    Stored, TokenPair,
};
use concord::{Base, Body, Retry, Secret};
use concord_testkit::Script;
use futures_util::{future::join_all, FutureExt};
use serde::{Deserialize, Serialize};
use std::error::Error as _;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

fn sec(n: u64) -> Duration {
    Duration::from_secs(n)
}
fn pair(access: &str, refresh: &str, life: u64) -> TokenPair {
    TokenPair::expires_in(access, refresh, sec(life))
}
fn expired() -> Stored {
    Stored::Ready(pair("old", "r0", 0))
}
fn base() -> Base {
    "https://data.test/v2".parse().unwrap()
}
fn cause(error: &concord::Error) -> &RotationError {
    RotationError::find(error).unwrap()
}
fn assert_renewal(error: &concord::Error, check: impl FnOnce(&RenewalError) -> bool) {
    match cause(error) {
        RotationError::Renewal(error) => assert!(check(error), "{error:?}"),
        other => panic!("{other:?}"),
    }
}

concord::api! {
    client Data {
        base "https://data.test/v2";
        read concord::Text;
        GET get() -> String { path "data"; }
        POST post(body: Body) -> String { body body; }
        POST marked(body: Body) -> String { body body; idempotent; }
        PUT forbidden() -> String { idempotent false; }
    }
}
#[derive(Deserialize)]
struct Reply {
    access: String,
    refresh: String,
    life: u64,
    #[serde(default)]
    no_expiry: bool,
}
concord::api! {
    client Tokens {
        base "https://tokens.test";
        POST exchange(refresh: Secret) -> Reply {
            path "token";
            body concord::form { "refresh_token" = refresh; }
            read concord::Json;
        }
    }
}

type Events = Arc<Mutex<Vec<String>>>;
#[derive(Clone)]
struct MemoryStore(Arc<Mutex<Memory>>);
struct Memory {
    record: Stored,
    events: Events,
    loads: usize,
    saves: usize,
    fail_save: Option<usize>,
    fail_saves: Vec<usize>,
    ambiguous: bool,
    fail_load: bool,
    panic_load: bool,
    panic_save: Option<usize>,
    load_delay: Duration,
    save_delay: Duration,
    gate_save: Option<(usize, Arc<tokio::sync::Notify>)>,
}
impl MemoryStore {
    fn new(record: Stored, events: Events) -> Self {
        Self(Arc::new(Mutex::new(Memory {
            record,
            events,
            loads: 0,
            saves: 0,
            fail_save: None,
            fail_saves: Vec::new(),
            ambiguous: false,
            fail_load: false,
            panic_load: false,
            panic_save: None,
            load_delay: Duration::ZERO,
            save_delay: Duration::ZERO,
            gate_save: None,
        })))
    }
    fn counts(&self) -> (usize, usize) {
        let s = self.0.lock().unwrap();
        (s.loads, s.saves)
    }
    fn ready(&self, access: &str, refresh: &str) {
        match &self.0.lock().unwrap().record {
            Stored::Ready(pair) => {
                assert_eq!(&*pair.access().expose(), access);
                assert_eq!(&*pair.refresh().expose(), refresh);
            }
            Stored::Refreshing => panic!("expected a saved pair"),
        }
    }
    fn marker(&self) {
        assert!(matches!(self.0.lock().unwrap().record, Stored::Refreshing));
    }
}
impl CredentialStore for MemoryStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        let (record, delay, fail, panics) = {
            let mut state = self.0.lock().unwrap();
            state.loads += 1;
            state.events.lock().unwrap().push("load".into());
            (
                state.record.clone(),
                state.load_delay,
                state.fail_load,
                state.panic_load,
            )
        };
        assert!(!panics, "private load panic");
        tokio::time::sleep(delay).await;
        if fail {
            Err(RenewalError::failed(std::io::Error::other(
                "private storage cause",
            )))
        } else {
            Ok(record)
        }
    }
    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        let (number, delay, panics, gate) = {
            let mut state = self.0.lock().unwrap();
            state.saves += 1;
            let event = match &record {
                Stored::Ready(pair) => format!("save {}", &*pair.refresh().expose()),
                Stored::Refreshing => "mark".into(),
            };
            state.events.lock().unwrap().push(event);
            (
                state.saves,
                state.save_delay,
                state.panic_save == Some(state.saves),
                state
                    .gate_save
                    .as_ref()
                    .filter(|(n, _)| *n == state.saves)
                    .map(|(_, gate)| gate.clone()),
            )
        };
        assert!(!panics, "private save panic");
        if let Some(gate) = gate {
            gate.notified().await;
        }
        tokio::time::sleep(delay).await;
        let mut state = self.0.lock().unwrap();
        let fail = state.fail_save == Some(number) || state.fail_saves.contains(&number);
        if !fail || state.ambiguous {
            state.record = record;
        }
        if fail {
            Err(RenewalError::failed(std::io::Error::other(
                "private storage cause",
            )))
        } else {
            Ok(())
        }
    }
}
struct Rig {
    store: MemoryStore,
    events: Events,
    tokens: Script,
    resource: Script,
}
impl Rig {
    fn new(record: Stored) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        Self {
            store: MemoryStore::new(record, events.clone()),
            events,
            tokens: Script::new(),
            resource: Script::new(),
        }
    }
    fn auth(&self) -> (Rotating, RotationDriver) {
        auth(self.store.clone(), self.tokens.clone(), self.events.clone())
    }
    fn client(&self, auth: Rotating) -> Data {
        Data::new()
            .with_transport(self.resource.clone())
            .with_middleware(auth)
    }
    fn issue(&self, access: &str, refresh: &str, delay: u64) {
        self.tokens
            .answer(200)
            .after(sec(delay))
            .read_upload()
            .body(serde_json::json!({"access":access,"refresh":refresh,"life":100}).to_string());
    }
    fn events(&self, expected: &[&str]) {
        assert_eq!(*self.events.lock().unwrap(), expected);
    }
    fn sent(&self, tokens: usize, resources: usize) {
        assert_eq!(self.tokens.requests().len(), tokens);
        assert_eq!(self.resource.requests().len(), resources);
    }
}
fn auth(store: impl CredentialStore, script: Script, events: Events) -> (Rotating, RotationDriver) {
    let tokens = Tokens::new()
        .with_transport(script)
        .with_idle_timeout(sec(3600));
    Rotating::new(&base(), store, move |exchange: Exchange| {
        let tokens = tokens.clone();
        let events = events.clone();
        async move {
            events
                .lock()
                .unwrap()
                .push(format!("exchange {}", &*exchange.refresh().expose()));
            let reply = tokens.exchange(exchange.refresh()).await?;
            Ok(if reply.no_expiry {
                TokenPair::new(reply.access, reply.refresh, None)
            } else {
                TokenPair::expires_in(reply.access, reply.refresh, sec(reply.life))
            })
        }
    })
}
fn header(script: &Script, at: usize, expected: &str) {
    let sent = script.requests();
    let header = sent[at].headers().get("authorization").unwrap();
    assert_eq!(header.to_str().unwrap(), expected);
    assert!(header.is_sensitive());
}
async fn ticks() {
    for _ in 0..24 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test(start_paused = true)]
async fn c12_7_rotating_starts_no_task() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 3);
    rig.resource.answer(200).body("ok");
    let metrics = tokio::runtime::Handle::current().metrics();
    let before = metrics.num_alive_tasks();
    let (auth, driver) = rig.auth();
    assert_eq!(metrics.num_alive_tasks(), before);
    let api = rig.client(auth);
    let (result, drained, during) = tokio::join!(
        async {
            let result = api.get().await;
            drop(api);
            result
        },
        driver,
        async {
            tokio::time::sleep(sec(1)).await;
            rig.sent(1, 0);
            metrics.num_alive_tasks()
        }
    );
    assert_eq!(result.unwrap(), "ok");
    drained.unwrap();
    rig.store.ready("a1", "r1");
    assert_eq!(
        during, before,
        "the application polls the in-flight exchange"
    );
    assert_eq!(metrics.num_alive_tasks(), before);
}

#[tokio::test(start_paused = true)]
async fn c12_7_initial_load_outlives_a_wait_deadline_while_demand_remains() {
    let rig = Rig::new(Stored::Ready(pair("a1", "r1", 100)));
    rig.store.0.lock().unwrap().load_delay = sec(5);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let mut driver = Box::pin(driver);
    let mut first = Box::pin(api.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    for _ in 0..6 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    assert_eq!(rig.store.counts(), (1, 0));

    // Keep the first waiter alive past its deadline. The store operation has
    // no timeout of its own; a later caller can still use this same load.
    tokio::time::advance(sec(4)).await;
    assert!(driver.as_mut().now_or_never().is_none());
    assert_eq!(rig.store.counts(), (1, 0));
    let mut second = Box::pin(api.get().send());
    assert!(second.as_mut().now_or_never().is_none());
    let ((), drained) = tokio::join!(
        async {
            assert_eq!(second.await.unwrap(), "ok");
            assert_renewal(&first.await.unwrap_err(), |e| {
                matches!(e, RenewalError::TimedOut(_))
            });
            drop(api);
        },
        driver
    );
    drained.unwrap();
    assert_eq!(rig.store.counts(), (1, 0));
    rig.sent(0, 1);
    header(&rig.resource, 0, "Bearer a1");
}

#[tokio::test(start_paused = true)]
async fn c12_8_unsaved_debug_hides_the_pair_and_cause() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().panic_save = Some(2);
    rig.issue("PRIVATE_ACCESS", "PRIVATE_REFRESH", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let (result, returned) = tokio::join!(
        async {
            let result = api.get().await;
            drop(api);
            result
        },
        driver
    );
    assert!(result.is_err());
    let error = returned.unwrap_err();
    assert!(matches!(&error, RotationError::Unsaved { pair, .. }
        if &*pair.access().expose() == "PRIVATE_ACCESS"
        && &*pair.refresh().expose() == "PRIVATE_REFRESH"));
    assert_eq!(format!("{error:?}"), "RotationError::Unsaved");
    assert_eq!(
        error.to_string(),
        "the replacement credential must be saved by the application"
    );
    assert!(error.source().is_some());
}

#[tokio::test(start_paused = true)]
async fn c12_6_simultaneous_calls_share_exchange_and_wait_for_durable_save() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().save_delay = sec(1);
    rig.issue("a1", "r1", 3);
    for _ in 0..16 {
        rig.resource.answer(200).body("ok");
    }
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let start = tokio::time::Instant::now();
    let results = join_all((0..16).map(|_| api.get().send())).await;
    assert!(results.into_iter().all(|result| result.unwrap() == "ok"));
    rig.sent(1, 16);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.store.ready("a1", "r1");
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
    assert!(rig
        .resource
        .requests()
        .iter()
        .all(|request| request.at() - start == sec(5)));
    for n in 0..16 {
        header(&rig.resource, n, "Bearer a1");
    }
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_next_exchange_uses_the_saved_replacement() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 0);
    rig.issue("a2", "r2", 0);
    rig.resource.answer(200).body("one");
    rig.resource.answer(200).body("two");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.early_by(sec(30)));
    let task = tokio::spawn(driver);
    api.get().await.unwrap();
    tokio::time::sleep(sec(70)).await;
    api.get().await.unwrap();
    rig.sent(2, 2);
    assert_eq!(rig.store.counts(), (1, 4));
    rig.store.ready("a2", "r2");
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r1",
        "mark",
        "exchange r1",
        "save r2",
    ]);
    let requests = rig.tokens.requests();
    assert_eq!(requests[0].upload().bytes(), b"refresh_token=r0");
    assert_eq!(requests[1].upload().bytes(), b"refresh_token=r1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_failed_intent_reloads_and_never_exchanges_before_acknowledgement() {
    for ambiguous in [false, true] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.fail_save = Some(1);
            s.ambiguous = ambiguous;
        }
        rig.issue("a1", "r1", 0);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Save(_)
        ));
        rig.sent(0, 0);
        assert_eq!(rig.store.counts(), (1, 1));
        rig.events(&["load", "mark"]);
        if ambiguous {
            assert!(matches!(
                cause(&api.get().await.unwrap_err()),
                RotationError::NeedsReauthorization
            ));
            rig.store.marker();
            rig.sent(0, 0);
            assert_eq!(rig.store.counts(), (2, 1));
            rig.events(&["load", "mark", "load"]);
        } else {
            api.get().await.unwrap();
            rig.sent(1, 1);
            assert_eq!(rig.store.counts(), (2, 3));
            rig.events(&["load", "mark", "load", "mark", "exchange r0", "save r1"]);
        }
        drop(api);
        let _ = task.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_failed_final_save_reloads_and_retries_only_that_pair() {
    for ambiguous in [false, true] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.fail_save = Some(2);
            s.ambiguous = ambiguous;
        }
        rig.issue("a1", "r1", 0);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        for result in join_all((0..8).map(|_| api.get().send())).await {
            assert!(matches!(
                cause(&result.unwrap_err()),
                RotationError::Save(_)
            ));
        }
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (1, 2));
        api.get().await.unwrap();
        rig.sent(1, 1);
        rig.store.ready("a1", "r1");
        assert_eq!(rig.store.counts(), (2, 3));
        rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
        drop(api);
        task.await.unwrap().unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_restart_after_failed_save_obeys_the_durable_record() {
    for ambiguous in [false, true] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.fail_saves = vec![2, 3];
            s.ambiguous = ambiguous;
        }
        rig.issue("a1", "r1", 0);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Save(_)
        ));
        drop(api);
        let result = task.await.unwrap();
        let replacement = returned_pair(&result).expect("the driver must return its unsaved pair");
        assert_eq!(&*replacement.refresh().expose(), "r1");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        if ambiguous {
            api.get().await.unwrap();
            rig.sent(1, 1);
        } else {
            assert!(matches!(
                cause(&api.get().await.unwrap_err()),
                RotationError::NeedsReauthorization
            ));
            rig.sent(1, 0);
        }
        assert_eq!(rig.store.counts(), (3, 3));
        rig.events(&[
            "load",
            "mark",
            "exchange r0",
            "save r1",
            "load",
            "save r1",
            "load",
        ]);
        drop(api);
        let _ = task.await.unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_failed_exchange_and_invalid_replacement_never_reuse_old_refresh() {
    for reply in ["error", "empty", "unchanged"] {
        let rig = Rig::new(expired());
        if reply == "error" {
            rig.tokens.answer(500).body("private provider text");
        } else {
            rig.issue("a1", if reply == "empty" { "" } else { "r0" }, 0);
        }
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        let error = api.get().await.unwrap_err();
        if reply == "error" {
            assert_renewal(&error, |e| matches!(e, RenewalError::Failed(_)));
        } else {
            assert!(matches!(cause(&error), RotationError::InvalidRefreshToken));
        }
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::NeedsReauthorization
        ));
        rig.store.marker();
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (1, 1));
        rig.events(&["load", "mark", "exchange r0"]);
        drop(api);
        assert!(task.await.unwrap().is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_unusable_access_still_saves_valid_refresh() {
    for (access, no_expiry, life) in [
        ("bad\nheader", false, 100),
        ("a1", true, 100),
        ("a1", false, 0),
    ] {
        let rig = Rig::new(expired());
        rig.tokens.answer(200).body(
            serde_json::json!({"access":access,"refresh":"r1","life":life,"no_expiry":no_expiry})
                .to_string(),
        );
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        let error = api.get().await.unwrap_err();
        assert_renewal(&error, |e| {
            if access.contains('\n') {
                matches!(e, RenewalError::InvalidHeader(_))
            } else {
                matches!(e, RenewalError::Expired)
            }
        });
        rig.store.ready(access, "r1");
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (1, 2));
        rig.events(&["load", "mark", "exchange r0", "save r1"]);
        drop(api);
        let _ = task.await.unwrap();
    }
    let overflow = TokenPair::expires_in("a", "r", Duration::MAX);
    assert!(overflow.expires_at().is_none());
    assert_eq!(&*overflow.refresh().expose(), "r");
}

#[tokio::test(start_paused = true)]
async fn c12_7_unpolled_driver_and_dropped_driver_have_distinct_errors() {
    let rig = Rig::new(expired());
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let start = tokio::time::Instant::now();
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NotPolled
    ));
    assert_eq!(start.elapsed(), sec(2));
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (0, 0));
    rig.events(&[]);
    drop(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Stopped
    ));
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (0, 0));
}

#[tokio::test(start_paused = true)]
async fn c12_7_wait_timeout_never_cancels_exchange_or_save() {
    for save_delay in [0, 2] {
        let rig = Rig::new(expired());
        rig.store.0.lock().unwrap().save_delay = sec(save_delay);
        rig.issue("a1", "r1", 5);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth.renewal_timeout(sec(3)));
        let task = tokio::spawn(driver);
        let error = api.get().await.unwrap_err();
        assert_renewal(&error, |e| matches!(e, RenewalError::TimedOut(_)));
        rig.sent(1, 0);
        tokio::time::sleep(sec(10)).await;
        rig.store.ready("a1", "r1");
        api.get().await.unwrap();
        rig.sent(1, 1);
        assert_eq!(rig.store.counts(), (1, 2));
        rig.events(&["load", "mark", "exchange r0", "save r1"]);
        drop(api);
        task.await.unwrap().unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_7_all_resource_waiters_drop_but_driver_saves_and_drains() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 5);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    rig.sent(1, 0);
    drop(call);
    drop(api);
    task.await.unwrap().unwrap();
    rig.store.ready("a1", "r1");
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_7_resource_call_timeout_does_not_cancel_driver() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 5);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(api
        .get()
        .with_timeout(sec(1))
        .await
        .unwrap_err()
        .is_timeout());
    rig.sent(1, 0);
    api.get().await.unwrap();
    rig.sent(1, 1);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_dropping_driver_during_exchange_stops_every_clone_without_resending() {
    let rig = Rig::new(expired());
    rig.tokens.never();
    let (auth, driver) = rig.auth();
    let other = auth.clone();
    let api = rig.client(auth);
    let mut driver = Box::pin(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    for _ in 0..12 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    rig.sent(1, 0);
    drop(driver);
    assert!(matches!(
        cause(&call.await.unwrap_err()),
        RotationError::Stopped
    ));
    assert!(matches!(
        cause(&rig.client(other).get().await.unwrap_err()),
        RotationError::Stopped
    ));
    rig.store.marker();
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 1));
}

#[tokio::test(start_paused = true)]
async fn c12_7_hung_store_bounds_wait_and_driver_drop_finishes_nothing() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().load_delay = sec(1000);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let mut driver = Box::pin(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    for _ in 0..6 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    tokio::time::advance(sec(2)).await;
    assert_renewal(&call.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (1, 0));
    drop(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Stopped
    ));
}

#[tokio::test(start_paused = true)]
async fn c12_7_deadline_wins_over_a_result_completed_before_late_poll() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    rig.store.ready("a1", "r1");
    tokio::time::advance(sec(2)).await;
    assert_renewal(&call.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_late_waiter_and_slow_save_never_send_expired_access() {
    let rig = Rig::new(expired());
    rig.tokens
        .answer(200)
        .body(r#"{"access":"short","refresh":"r1","life":1}"#);
    rig.issue("a2", "r2", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    tokio::time::advance(sec(2)).await;
    call.await.unwrap();
    rig.sent(2, 1);
    header(&rig.resource, 0, "Bearer a2");
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r1",
        "mark",
        "exchange r1",
        "save r2",
    ]);
    assert_eq!(rig.store.counts(), (1, 4));
    drop(api);
    task.await.unwrap().unwrap();

    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().save_delay = sec(2);
    rig.tokens
        .answer(200)
        .body(r#"{"access":"short","refresh":"r1","life":1}"#);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Expired)
    });
    rig.store.ready("short", "r1");
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_8_origin_and_static_policy_are_checked_before_storage() {
    for url in [
        "https://other.test",
        "http://data.test",
        "https://data.test:444",
    ] {
        let rig = Rig::new(expired());
        let (auth, driver) = rig.auth();
        let task = tokio::spawn(driver);
        let api = rig.client(auth).with_base(url.parse::<Base>().unwrap());
        assert_renewal(&api.get().await.unwrap_err(), |e| {
            matches!(e, RenewalError::WrongOrigin)
        });
        rig.sent(0, 0);
        assert_eq!(rig.store.counts(), (0, 0));
        drop(api);
        task.await.unwrap().unwrap();
    }
    for timeout in [Duration::ZERO, Duration::MAX] {
        let rig = Rig::new(expired());
        let (auth, driver) = rig.auth();
        let task = tokio::spawn(driver);
        let api = rig.client(auth.renewal_timeout(timeout));
        assert_renewal(&api.get().await.unwrap_err(), |e| {
            matches!(e, RenewalError::InvalidPolicy | RenewalError::TimeOverflow)
        });
        rig.sent(0, 0);
        assert_eq!(rig.store.counts(), (0, 0));
        drop(api);
        task.await.unwrap().unwrap();
    }
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig
        .client(auth)
        .with_base("https://DATA.test:443/other".parse::<Base>().unwrap());
    let task = tokio::spawn(driver);
    api.get().await.unwrap();
    rig.sent(0, 1);
    assert_eq!(rig.store.counts(), (1, 0));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_401_uses_engine_attempts_and_rejects_only_its_generation() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
    rig.issue("a1", "r1", 0);
    rig.resource.answer(401).after(sec(5));
    rig.resource.answer(401);
    rig.resource.answer(200).body("b");
    rig.resource.answer(200).body("a");
    let (auth, driver) = rig.auth();
    let api = rig
        .client(auth)
        .with_retry(Retry::none().tries(2).when(after_401));
    let task = tokio::spawn(driver);
    let (a, b) = tokio::join!(api.get().send(), api.get().send());
    assert!(a.is_ok() && b.is_ok());
    rig.sent(1, 4);
    assert_eq!(rig.store.counts(), (1, 2));
    assert_eq!(
        rig.resource
            .requests()
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 1, 2, 2]
    );
    header(&rig.resource, 0, "Bearer old");
    header(&rig.resource, 1, "Bearer old");
    header(&rig.resource, 2, "Bearer a1");
    header(&rig.resource, 3, "Bearer a1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_middleware_never_resends_and_engine_preserves_repeatability_checks() {
    for kind in [
        "no_retry",
        "post",
        "forbidden",
        "stream",
        "marked",
        "second_401",
    ] {
        let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
        rig.issue("a1", "r1", 0);
        rig.resource.answer(401).read_upload();
        rig.resource
            .answer(if kind == "second_401" { 401 } else { 200 })
            .read_upload()
            .body("ok");
        let (auth, driver) = rig.auth();
        let mut api = rig.client(auth);
        let task = tokio::spawn(driver);
        if kind != "no_retry" {
            api = api.with_retry(Retry::none().tries(2).when(after_401));
        }
        let result = match kind {
            "post" => api.post(Body::from("payload")).send().await,
            "forbidden" => api.forbidden().send().await,
            "stream" => {
                api.marked(Body::stream(futures_util::stream::iter([Ok::<
                    _,
                    std::io::Error,
                >(
                    concord::bytes::Bytes::from_static(b"payload"),
                )])))
                .send()
                .await
            }
            "marked" => api.marked(Body::from("payload")).send().await,
            _ => api.get().send().await,
        };
        let repeated = matches!(kind, "marked" | "second_401");
        if kind == "marked" {
            assert_eq!(result.unwrap(), "ok");
        } else {
            let error = result.unwrap_err();
            assert!(error.is_status(401));
            assert_eq!(error.tries(), if repeated { 2 } else { 1 });
        }
        rig.sent(usize::from(repeated), if repeated { 2 } else { 1 });
        assert_eq!(rig.store.counts(), (1, if repeated { 2 } else { 0 }));
        assert_eq!(rig.resource.unused(), if repeated { 0 } else { 1 });
        if repeated {
            assert_eq!(
                rig.resource
                    .requests()
                    .iter()
                    .map(|r| r.info().number())
                    .collect::<Vec<_>>(),
                [1, 2]
            );
        }
        if kind == "marked" {
            let requests = rig.resource.requests();
            assert_eq!(requests[0].upload().bytes(), requests[1].upload().bytes());
        }
        drop(api);
        task.await.unwrap().unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_secret_snapshots_and_custom_header_never_print_values() {
    let access: Secret = "private-access".into();
    let refresh: Secret = "private-refresh".into();
    let value = TokenPair::expires_in(&access, &refresh, sec(100));
    access.replace("changed-access");
    refresh.replace("changed-refresh");
    value.access().replace("changed-accessor");
    value.refresh().replace("changed-accessor");
    assert_eq!(&*value.access().expose(), "private-access");
    assert_eq!(&*value.refresh().expose(), "private-refresh");
    let rig = Rig::new(Stored::Ready(value.clone()));
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    for text in [
        format!("{value:?}"),
        format!("{:?}", Stored::Ready(value)),
        format!("{auth:?}"),
        format!("{driver:?}"),
    ] {
        assert!(!text.contains("private-"));
    }
    let auth = auth.header(concord::http::HeaderName::from_static("x-token"), |token| {
        concord::http::HeaderValue::from_str(&token.expose())
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    api.get().await.unwrap();
    let requests = rig.resource.requests();
    let h = requests[0].headers().get("x-token").unwrap();
    assert_eq!(h.to_str().unwrap(), "private-access");
    assert!(h.is_sensitive());
    assert!(!format!("{h:?}").contains("private-access"));
    rig.sent(0, 1);
    assert_eq!(rig.store.counts(), (1, 0));
    rig.events(&["load"]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_overflow_expiry_is_saved_before_access_fails() {
    let value = TokenPair::expires_in("a1", "r1", Duration::MAX);
    assert!(value.expires_at().is_none());
    let rig = Rig::new(expired());
    let saved = value.clone();
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |_| {
        let saved = saved.clone();
        async move { Ok(saved) }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Expired)
    });
    rig.store.ready("a1", "r1");
    assert_eq!(rig.store.counts(), (1, 2));
    rig.sent(0, 0);
    rig.events(&["load", "mark", "save r1"]);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_7_store_panics_stop_the_instance_after_notifying_waiters() {
    for stage in [0, 1, 2] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.panic_load = stage == 0;
            s.panic_save = if stage == 0 { None } else { Some(stage) };
        }
        rig.issue("a1", "r1", 0);
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        for result in join_all((0..4).map(|_| api.get().send())).await {
            let error = result.unwrap_err();
            assert!(matches!(cause(&error), RotationError::Stopped));
            assert!(!format!("{error} {error:?}").contains("private"));
        }
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Stopped
        ));
        rig.sent(usize::from(stage == 2), 0);
        assert_eq!(rig.store.counts(), (1, stage));
        if stage == 0 {
            rig.events(&["load"]);
        }
        if stage == 1 {
            rig.events(&["load", "mark"]);
        }
        if stage == 2 {
            rig.events(&["load", "mark", "exchange r0", "save r1"]);
        }
        drop(api);
        assert!(task.await.unwrap().is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_exchange_and_header_panics_keep_safe_state_without_stranding_waiters() {
    let rig = Rig::new(expired());
    let invocations = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invocations.clone();
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |_| {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async {
            panic!("private exchange panic");
            #[allow(unreachable_code)]
            Ok(pair("a1", "r1", 100))
        }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    for result in join_all((0..4).map(|_| api.get().send())).await {
        assert_renewal(&result.unwrap_err(), |e| {
            matches!(e, RenewalError::Panicked)
        });
    }
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    assert_eq!(invocations.load(std::sync::atomic::Ordering::SeqCst), 1);
    rig.store.marker();
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (1, 1));
    drop(api);
    assert!(task.await.unwrap().is_err());

    let rig = Rig::new(Stored::Ready(pair("a1", "r1", 100)));
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let change = auth.clone();
    let api = rig.client(auth.header(concord::http::header::AUTHORIZATION, |_| {
        panic!("private formatter panic")
    }));
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Panicked)
    });
    drop(change.header(concord::http::header::AUTHORIZATION, |_| {
        Ok(concord::http::HeaderValue::from_static("recovered"))
    }));
    api.get().await.unwrap();
    rig.sent(0, 1);
    assert_eq!(rig.store.counts(), (1, 0));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_typed_causes_and_cyclic_error_search_do_not_print_values() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().fail_load = true;
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let error = api.get().await.unwrap_err();
    let rotation = cause(&error);
    assert!(matches!(
        rotation,
        RotationError::Load(RenewalError::Failed(_))
    ));
    assert!(rotation
        .source()
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<std::io::Error>()
        .is_some());
    assert!(RenewalError::find(&error).is_some());
    assert_eq!(format!("{rotation:?}"), "RotationError::Load");
    assert_eq!(rotation.to_string(), "the credential could not be loaded");
    assert!(!format!("{error} {error:?}").contains("private storage cause"));
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (1, 0));
    drop(api);
    assert!(task.await.unwrap().is_err());
    #[derive(Debug)]
    struct Cycle;
    impl std::fmt::Display for Cycle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("cycle")
        }
    }
    impl std::error::Error for Cycle {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(self)
        }
    }
    assert!(RotationError::find(&Cycle).is_none());
}

#[tokio::test(start_paused = true)]
async fn c12_8_header_insertion_failure_retains_the_original_cause() {
    let rig = Rig::new(Stored::Ready(pair("a1", "r1", 100)));
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let error = api
        .get()
        .edit_request(|request| {
            for n in 0..40000 {
                let name = concord::http::HeaderName::from_bytes(format!("x-fill-{n}").as_bytes())
                    .unwrap();
                if request
                    .headers_mut()
                    .try_insert(name, concord::http::HeaderValue::from_static("x"))
                    .is_err()
                {
                    break;
                }
            }
        })
        .await
        .unwrap_err();
    let renewal = RenewalError::find(&error)
        .unwrap_or_else(|| panic!("missing renewal source: {error:?} / {error}"));
    assert!(matches!(renewal, RenewalError::Failed(_)));
    assert!(renewal
        .source()
        .unwrap()
        .downcast_ref::<concord::http::header::MaxSizeReached>()
        .is_some());
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (1, 0));
    drop(api);
    task.await.unwrap().unwrap();
}

#[derive(Clone)]
struct FileStore {
    path: std::path::PathBuf,
    tokens: Option<Script>,
}
impl FileStore {
    fn trace(&self, event: &str) {
        use std::io::Write;
        let mut trace = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.path.with_extension("trace"))
            .unwrap();
        writeln!(trace, "{event}").unwrap();
        trace.sync_all().unwrap();
    }
}
#[derive(Serialize, Deserialize)]
enum FileRecord {
    Ready {
        access: String,
        refresh: String,
        expiry: Option<SystemTime>,
    },
    Refreshing,
}
impl CredentialStore for FileStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        self.trace("load");
        let bytes = std::fs::read(&self.path).map_err(RenewalError::failed)?;
        Ok(
            match serde_json::from_slice::<FileRecord>(&bytes).map_err(RenewalError::failed)? {
                FileRecord::Ready {
                    access,
                    refresh,
                    expiry,
                } => Stored::Ready(TokenPair::new(access, refresh, expiry)),
                FileRecord::Refreshing => Stored::Refreshing,
            },
        )
    }
    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        use std::io::Write;
        let record = match record {
            Stored::Ready(pair) => {
                self.trace("save");
                if std::env::var_os("CONCORD_ROTATION_CRASH_DIR").is_some() {
                    let requests = self.tokens.as_ref().unwrap().requests();
                    assert_eq!(requests.len(), 1);
                    assert_eq!(requests[0].upload().bytes(), b"refresh_token=r0");
                    assert_eq!(requests[0].info().number(), 1);
                    std::process::exit(77);
                }
                FileRecord::Ready {
                    access: pair.access().expose().to_string(),
                    refresh: pair.refresh().expose().to_string(),
                    expiry: pair.expires_at(),
                }
            }
            Stored::Refreshing => {
                self.trace("mark");
                FileRecord::Refreshing
            }
        };
        let bytes = serde_json::to_vec(&record).map_err(RenewalError::failed)?;
        let temporary = self.path.with_extension("pending");
        let mut file = std::fs::File::create(&temporary).map_err(RenewalError::failed)?;
        file.write_all(&bytes).map_err(RenewalError::failed)?;
        file.sync_all().map_err(RenewalError::failed)?;
        std::fs::rename(temporary, &self.path).map_err(RenewalError::failed)?;
        std::fs::File::open(self.path.parent().unwrap())
            .and_then(|f| f.sync_all())
            .map_err(RenewalError::failed)?;
        Ok(())
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_crash_child() {
    let Some(directory) = std::env::var_os("CONCORD_ROTATION_CRASH_DIR") else {
        return;
    };
    let tokens = Script::new();
    let store = FileStore {
        path: std::path::PathBuf::from(directory).join("account.json"),
        tokens: Some(tokens.clone()),
    };
    tokens
        .answer(200)
        .read_upload()
        .body(r#"{"access":"a1","refresh":"r1","life":100}"#);
    let (auth, driver) = auth(store, tokens, Arc::new(Mutex::new(Vec::new())));
    let api = Data::new()
        .with_middleware(auth)
        .with_transport(Script::new());
    let task = tokio::spawn(driver);
    let _ = api.get().await;
    drop(api);
    let _ = task.await;
    panic!("the child should have exited before saving the replacement");
}

#[tokio::test(start_paused = true)]
async fn c12_6_process_death_between_issuance_and_save_leaves_durable_intent() {
    let directory =
        std::env::temp_dir().join(format!("concord-rotation-crash-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let store = FileStore {
        path: directory.join("account.json"),
        tokens: None,
    };
    store.save(expired()).await.unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "c12_6_crash_child", "--nocapture"])
        .env("CONCORD_ROTATION_CRASH_DIR", &directory)
        .env_remove("NEXTEST_RUN_ID")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(77),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(matches!(store.load().await.unwrap(), Stored::Refreshing));
    let events = Arc::new(Mutex::new(Vec::new()));
    let tokens = Script::new();
    let resources = Script::new();
    let (auth, driver) = auth(store, tokens.clone(), events.clone());
    let api = Data::new()
        .with_middleware(auth)
        .with_transport(resources.clone());
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    assert!(events.lock().unwrap().is_empty());
    assert_eq!(tokens.requests().len(), 0);
    assert_eq!(resources.requests().len(), 0);
    drop(api);
    assert!(task.await.unwrap().is_err());
    let trace = std::fs::read_to_string(directory.join("account.trace")).unwrap();
    assert_eq!(
        trace.lines().collect::<Vec<_>>(),
        ["save", "load", "mark", "save", "load", "load"]
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_token_client_timeout_is_an_uncertain_exchange_not_a_driver_timeout() {
    let rig = Rig::new(expired());
    rig.tokens.never();
    let tokens = Tokens::new()
        .with_transport(rig.tokens.clone())
        .with_timeout(sec(2));
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |exchange: Exchange| {
        let tokens = tokens.clone();
        async move {
            let reply = tokens.exchange(exchange.refresh()).await?;
            Ok(TokenPair::expires_in(
                reply.access,
                reply.refresh,
                sec(reply.life),
            ))
        }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let error = api.get().await.unwrap_err();
    assert_renewal(&error, |e| matches!(e, RenewalError::Failed(_)));
    let endpoint_error = RenewalError::find(&error)
        .unwrap()
        .source()
        .unwrap()
        .downcast_ref::<concord::Error>()
        .unwrap();
    assert!(endpoint_error.is_timeout());
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    rig.store.marker();
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 1));
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_7_driver_drop_during_save_cancels_the_future_and_never_publishes_early() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().save_delay = sec(5);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let mut driver = Box::pin(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    for _ in 0..8 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    assert_eq!(rig.store.counts(), (1, 1));
    rig.sent(0, 0);
    tokio::time::advance(sec(5)).await;
    for _ in 0..12 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    assert_eq!(rig.store.counts(), (1, 2));
    rig.sent(1, 0);
    rig.store.marker();
    drop(driver);
    assert!(matches!(
        cause(&call.await.unwrap_err()),
        RotationError::Stopped
    ));
    tokio::time::advance(sec(100)).await;
    rig.store.marker();
    rig.sent(1, 0);
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_pending_save_retry_keeps_the_original_access_deadline() {
    let rig = Rig::new(expired());
    {
        let mut s = rig.store.0.lock().unwrap();
        s.save_delay = sec(2);
        s.fail_save = Some(2);
    }
    rig.tokens
        .answer(200)
        .body(r#"{"access":"short","refresh":"r1","life":1}"#);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Save(_)
    ));
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Expired)
    });
    rig.store.ready("short", "r1");
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (2, 3));
    rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_8_policy_is_rechecked_after_saving_before_access_is_installed() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().save_delay = sec(1);
    rig.tokens
        .answer(200)
        .body(r#"{"access":"short","refresh":"r1","life":4}"#);
    let (auth, driver) = rig.auth();
    let change = auth.clone();
    let api = rig.client(auth);
    let mut driver = Box::pin(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    for _ in 0..8 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    tokio::time::advance(sec(1)).await;
    for _ in 0..12 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    assert_eq!(rig.store.counts(), (1, 2));
    drop(change.early_by(sec(10)));
    tokio::time::advance(sec(1)).await;
    for _ in 0..6 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    assert_renewal(&call.await.unwrap_err(), |e| {
        matches!(e, RenewalError::Expired)
    });
    rig.store.ready("short", "r1");
    rig.sent(1, 0);
    drop(driver);
}

#[tokio::test(start_paused = true)]
async fn c12_7_cancelled_demand_starts_no_store_operation_when_driver_is_later_polled() {
    let rig = Rig::new(expired());
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    drop(call);
    let mut driver = Box::pin(driver);
    assert!(driver.as_mut().now_or_never().is_none());
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (0, 0));
    rig.events(&[]);
    drop(api);
    driver.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_each_wait_distinguishes_a_stalled_driver_from_an_acknowledged_wait() {
    let rig = Rig::new(expired());
    rig.tokens.never();
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let mut driver = Box::pin(driver);
    let mut first = Box::pin(api.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    for _ in 0..10 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    rig.sent(1, 0);
    let mut second = Box::pin(api.get().send());
    assert!(second.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(2)).await;
    assert_renewal(&first.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    assert!(matches!(
        cause(&second.await.unwrap_err()),
        RotationError::NotPolled
    ));
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 1));
    rig.events(&["load", "mark", "exchange r0"]);
    drop(driver);
}

#[tokio::test(start_paused = true)]
async fn c12_8_reentrant_formatter_policy_change_is_checked_before_dispatch() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let holder = Arc::new(std::sync::OnceLock::<std::sync::Weak<Rotating>>::new());
    let target = holder.clone();
    let auth = Arc::new(auth.header(concord::http::header::AUTHORIZATION, move |_| {
        drop(
            target
                .get()
                .unwrap()
                .upgrade()
                .unwrap()
                .as_ref()
                .clone()
                .early_by(sec(200)),
        );
        Ok(concord::http::HeaderValue::from_static("must-not-be-sent"))
    }));
    holder.set(Arc::downgrade(&auth)).unwrap();
    let api = rig.client(auth.as_ref().clone());
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Expired)
    });
    rig.store.ready("a1", "r1");
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
    drop(auth);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_8_direct_recursive_exchange_is_refused_instead_of_waiting_on_itself() {
    let rig = Rig::new(expired());
    let holder = Arc::new(std::sync::OnceLock::<std::sync::Weak<Rotating>>::new());
    let target = holder.clone();
    let observed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let saw = observed.clone();
    let script = rig.resource.clone();
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |_| {
        let auth = target.get().unwrap().upgrade().unwrap();
        let script = script.clone();
        let saw = saw.clone();
        async move {
            let request = concord::reqwest::Request::new(
                concord::http::Method::GET,
                "https://data.test/v2/data".parse().unwrap(),
            );
            let error = concord::Middleware::handle(
                auth.as_ref(),
                request,
                concord::transport::TryInfo::new("nested.get", 1),
                concord::Next::to(&script),
            )
            .await
            .unwrap_err();
            let nested = RotationError::find(&error).unwrap();
            saw.store(
                matches!(nested, RotationError::Renewal(RenewalError::InvalidPolicy)),
                std::sync::atomic::Ordering::SeqCst,
            );
            Err::<TokenPair, _>(RenewalError::failed(error))
        }
    });
    let auth = Arc::new(auth);
    holder.set(Arc::downgrade(&auth)).unwrap();
    let api = rig.client(auth.as_ref().clone());
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Failed(_))
    });
    assert!(observed.load(std::sync::atomic::Ordering::SeqCst));
    rig.sent(0, 0);
    rig.store.marker();
    assert_eq!(rig.store.counts(), (1, 1));
    drop(api);
    drop(auth);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_6_independent_owners_show_that_load_save_is_not_a_cross_process_lease() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().load_delay = sec(1);
    rig.issue("a1", "r1", 1);
    rig.tokens.answer(400).read_upload();
    rig.resource.answer(200).body("ok");
    let (a, ad) = rig.auth();
    let (b, bd) = rig.auth();
    let a = rig.client(a);
    let b = rig.client(b);
    let ad = tokio::spawn(ad);
    let bd = tokio::spawn(bd);
    let (left, right) = tokio::join!(a.get().send(), b.get().send());
    assert!(left.is_ok());
    assert!(right.is_err());
    rig.sent(2, 1);
    assert_eq!(rig.store.counts(), (2, 3));
    assert!(rig
        .tokens
        .requests()
        .iter()
        .all(|request| request.upload().bytes() == b"refresh_token=r0"));
    rig.events(&[
        "load",
        "load",
        "mark",
        "mark",
        "exchange r0",
        "exchange r0",
        "save r1",
    ]);
    drop(a);
    drop(b);
    ad.await.unwrap().unwrap();
    assert!(bd.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_7_joining_an_active_exchange_wakes_a_polled_driver() {
    let rig = Rig::new(expired());
    rig.tokens.never();
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let task = tokio::spawn(driver);
    let started = tokio::time::Instant::now();
    let mut first = Box::pin(api.get().send());
    assert!(first.as_mut().now_or_never().is_none());
    ticks().await;
    rig.sent(1, 0);
    tokio::time::advance(sec(1)).await;
    let mut second = Box::pin(api.get().send());
    assert!(second.as_mut().now_or_never().is_none());
    ticks().await;
    assert_renewal(&first.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    assert_eq!(started.elapsed(), sec(2));
    // The second call has its own full allowance and the task acknowledged it,
    // even though no token response or store operation became ready.
    assert_renewal(&second.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    assert_eq!(started.elapsed(), sec(3));
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 1));
    rig.events(&["load", "mark", "exchange r0"]);
    task.abort();
    let _ = task.await;
}

#[tokio::test(start_paused = true)]
async fn c12_8_replaced_header_drop_cannot_dispatch_expired_access_or_overrun_wait() {
    struct Advance {
        by: Duration,
        drops: Arc<std::sync::atomic::AtomicUsize>,
    }
    impl AsRef<[u8]> for Advance {
        fn as_ref(&self) -> &[u8] {
            b"old header"
        }
    }
    impl Drop for Advance {
        fn drop(&mut self) {
            self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            // Advance changes virtual time on its first poll, then yields.
            let _ = tokio::time::advance(self.by).now_or_never();
        }
    }
    for expires_first in [true, false] {
        let rig = Rig::new(Stored::Ready(pair(
            "old",
            "r0",
            if expires_first { 1 } else { 100 },
        )));
        if expires_first {
            rig.issue("a1", "r1", 0);
            rig.resource.answer(200);
        }
        let (auth, driver) = rig.auth();
        let task = tokio::spawn(driver);
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let owner = Advance {
            by: sec(if expires_first { 2 } else { 31 }),
            drops: drops.clone(),
        };
        let old_header =
            concord::http::HeaderValue::from_maybe_shared(concord::bytes::Bytes::from_owner(owner))
                .unwrap();
        let mut request = concord::reqwest::Request::new(
            concord::http::Method::GET,
            "https://data.test/v2/data".parse().unwrap(),
        );
        request
            .headers_mut()
            .insert(concord::http::header::AUTHORIZATION, old_header);
        let result = concord::Middleware::handle(
            &auth,
            request,
            concord::transport::TryInfo::new("data.get", 1),
            concord::Next::to(&rig.resource),
        )
        .await;
        assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
        if expires_first {
            assert!(result.is_ok());
            rig.sent(1, 1);
            header(&rig.resource, 0, "Bearer a1");
            assert_eq!(rig.store.counts(), (1, 2));
            rig.events(&["load", "mark", "exchange r0", "save r1"]);
        } else {
            let error = result.unwrap_err();
            assert!(matches!(
                RotationError::find(&error),
                Some(RotationError::Renewal(RenewalError::TimedOut(_)))
            ));
            rig.sent(0, 0);
            assert_eq!(rig.store.counts(), (1, 0));
            rig.events(&["load"]);
        }
        drop(auth);
        task.await.unwrap().unwrap();
    }
}

struct CompletedExchangeDrop {
    work: std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<TokenPair, RenewalError>> + Send>,
    >,
    complete: bool,
}
impl std::future::Future for CompletedExchangeDrop {
    type Output = Result<TokenPair, RenewalError>;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let result = self.work.as_mut().poll(cx);
        self.complete = result.is_ready();
        result
    }
}
impl Drop for CompletedExchangeDrop {
    fn drop(&mut self) {
        assert!(!self.complete, "completed exchange destructor");
    }
}
fn stored_refresh(store: &MemoryStore) -> Option<String> {
    match &store.0.lock().unwrap().record {
        Stored::Ready(pair) => Some(pair.refresh().expose().to_string()),
        Stored::Refreshing => None,
    }
}
// The conservation assertion observes only public recovery data, never internals.
fn returned_pair(result: &Result<(), RotationError>) -> Option<TokenPair> {
    match result {
        Err(RotationError::Unsaved { pair, .. }) => Some(pair.clone()),
        _ => None,
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_completed_exchange_drop_preserves_the_returned_pair() {
    let rig = Rig::new(expired());
    rig.issue("a1", "r1", 0);
    let tokens = Tokens::new().with_transport(rig.tokens.clone());
    let events = rig.events.clone();
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |input: Exchange| {
        let tokens = tokens.clone();
        let events = events.clone();
        CompletedExchangeDrop {
            work: Box::pin(async move {
                events.lock().unwrap().push("exchange r0".into());
                let reply = tokens.exchange(input.refresh()).await?;
                Ok(TokenPair::expires_in(
                    reply.access,
                    reply.refresh,
                    sec(reply.life),
                ))
            }),
            complete: false,
        }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let _ = api.get().await;
    drop(api);
    let result = task.await.unwrap();
    assert!(
        stored_refresh(&rig.store).as_deref() == Some("r1") || returned_pair(&result).is_some(),
        "Ready pair lost during exchange cleanup: {result:?}"
    );
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_graceful_drain_retries_a_failed_final_save() {
    let rig = Rig::new(expired());
    let gate = Arc::new(tokio::sync::Notify::new());
    {
        let mut s = rig.store.0.lock().unwrap();
        s.fail_save = Some(2);
        s.gate_save = Some((2, gate.clone()));
    }
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    assert_eq!(rig.store.counts(), (1, 2));
    drop(call);
    drop(api);
    gate.notify_one();
    let result = task.await.unwrap();
    assert!(
        result.is_ok(),
        "graceful drain abandoned its pending replacement: {result:?}"
    );
    rig.store.ready("a1", "r1");
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (2, 3));
    rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_store_panic_returns_the_unsaved_replacement() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().panic_save = Some(2);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let _ = api.get().await;
    drop(api);
    let result = task.await.unwrap();
    assert!(
        stored_refresh(&rig.store).as_deref() == Some("r1") || returned_pair(&result).is_some(),
        "store panic discarded the issued replacement: {result:?}"
    );
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (1, 2));
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_6_matching_reload_still_requires_a_durable_save() {
    let rig = Rig::new(expired());
    {
        let mut s = rig.store.0.lock().unwrap();
        s.fail_save = Some(2);
        s.ambiguous = true;
    }
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Save(_)
    ));
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.store.0.lock().unwrap().gate_save = Some((3, gate.clone()));
    let mut second = Box::pin(api.get().send());
    let early = second.as_mut().now_or_never();
    ticks().await;
    assert!(early.is_none());
    assert_eq!(
        rig.store.counts(),
        (2, 3),
        "visible reload bypassed durable acknowledgement"
    );
    rig.sent(1, 0);
    gate.notify_one();
    second.await.unwrap();
    rig.sent(1, 1);
    rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_late_driver_poll_does_not_change_not_polled() {
    let rig = Rig::new(expired());
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    tokio::time::advance(sec(3)).await;
    let mut driver = Box::pin(driver);
    assert!(driver.as_mut().now_or_never().is_none());
    let error = call.await.unwrap_err();
    assert!(
        matches!(cause(&error), RotationError::NotPolled),
        "late poll retroactively acknowledged an expired wait: {error:?}"
    );
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (0, 0));
    drop(driver);
}

#[tokio::test(start_paused = true)]
async fn c12_7_timeout_during_final_save_does_not_cancel_it() {
    let rig = Rig::new(expired());
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.store.0.lock().unwrap().gate_save = Some((2, gate.clone()));
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.renewal_timeout(sec(2)));
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    assert_eq!(rig.store.counts(), (1, 2));
    rig.store.marker();
    rig.sent(1, 0);
    assert_renewal(&call.await.unwrap_err(), |e| {
        matches!(e, RenewalError::TimedOut(_))
    });
    assert_eq!(rig.store.counts(), (1, 2));
    gate.notify_one();
    drop(api);
    task.await.unwrap().unwrap();
    rig.store.ready("a1", "r1");
    rig.sent(1, 0);
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
}

#[tokio::test(start_paused = true)]
async fn c12_7_dropped_first_load_never_begins_the_mark() {
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().load_delay = sec(1000);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    assert_eq!(rig.store.counts(), (1, 0));
    drop(call);
    ticks().await;
    tokio::time::advance(sec(1001)).await;
    ticks().await;
    assert_eq!(
        rig.store.counts(),
        (1, 0),
        "load cancellation was treated as a begun renewal"
    );
    rig.sent(0, 0);
    rig.events(&["load"]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_empty_stored_refresh_is_refused_before_a_mark() {
    let rig = Rig::new(Stored::Ready(pair("old", "", 100)));
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::InvalidRefreshToken
    ));
    rig.sent(0, 0);
    assert_eq!(rig.store.counts(), (1, 0));
    rig.events(&["load"]);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_6_invalid_exchange_refresh_leaves_marker_and_requires_reauthorization() {
    for refresh in ["", "r0"] {
        let rig = Rig::new(expired());
        rig.issue("a1", refresh, 0);
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::InvalidRefreshToken
        ));
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::NeedsReauthorization
        ));
        rig.store.marker();
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (1, 1));
        rig.events(&["load", "mark", "exchange r0"]);
        drop(api);
        assert!(task.await.unwrap().is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_7_store_panic_stops_current_and_later_calls_and_reports_to_driver() {
    for phase in [0, 1, 2] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.panic_load = phase == 0;
            s.panic_save = if phase == 0 { None } else { Some(phase) };
        }
        rig.issue("a1", "r1", 0);
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Stopped
        ));
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Stopped
        ));
        let result = task.await.unwrap();
        if phase == 2 {
            assert_eq!(&*returned_pair(&result).unwrap().refresh().expose(), "r1");
        } else {
            assert!(matches!(result, Err(RotationError::Stopped)));
        }
        rig.sent(usize::from(phase == 2), 0);
        assert_eq!(rig.store.counts(), (1, phase));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_next_call_renews_with_saved_refresh_after_unusable_access() {
    for (access, no_expiry, life) in [
        ("bad\nheader", false, 100),
        ("a1", true, 100),
        ("a1", false, 0),
    ] {
        let rig = Rig::new(expired());
        rig.tokens.answer(200).read_upload().body(
            serde_json::json!({"access":access,"refresh":"r1","life":life,"no_expiry":no_expiry})
                .to_string(),
        );
        rig.issue("a2", "r2", 0);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = rig.auth();
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(api.get().await.is_err());
        rig.store.ready(access, "r1");
        rig.sent(1, 0);
        api.get().await.unwrap();
        rig.sent(2, 1);
        header(&rig.resource, 0, "Bearer a2");
        assert_eq!(rig.store.counts(), (1, 4));
        let requests = rig.tokens.requests();
        assert_eq!(requests[1].upload().bytes(), b"refresh_token=r1");
        rig.events(&[
            "load",
            "mark",
            "exchange r0",
            "save r1",
            "mark",
            "exchange r1",
            "save r2",
        ]);
        drop(api);
        task.await.unwrap().unwrap();
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_configuration_matches_renewed_shared_account_settings() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
    rig.tokens
        .answer(200)
        .read_upload()
        .body(r#"{"access":"a1","refresh":"r1","life":1000}"#);
    rig.resource.answer(200).body("old");
    rig.resource.answer(200).body("new");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.clone());
    let task = tokio::spawn(driver);
    drop(
        auth.clone()
            .header(concord::http::HeaderName::from_static("x-token"), |token| {
                concord::http::HeaderValue::from_str(&token.expose())
            }),
    );
    api.get().await.unwrap();
    assert_eq!(rig.resource.requests()[0].headers()["x-token"], "old");
    drop(auth.clone().early_by(sec(200)));
    api.get().await.unwrap();
    assert_eq!(rig.resource.requests()[1].headers()["x-token"], "a1");
    drop(auth.renewal_timeout(Duration::ZERO));
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::InvalidPolicy)
    });
    rig.sent(1, 2);
    assert_eq!(rig.store.counts(), (1, 2));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_relative_lifetime_follows_the_renewal_timer() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 5)));
    rig.issue("a1", "r1", 0);
    for _ in 0..3 {
        rig.resource.answer(200).body("ok");
    }
    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    api.get().await.unwrap();
    tokio::time::advance(sec(4)).await;
    api.get().await.unwrap();
    rig.sent(0, 2);
    tokio::time::advance(sec(1)).await;
    api.get().await.unwrap();
    rig.sent(1, 3);
    assert_eq!(rig.store.counts(), (1, 2));
    header(&rig.resource, 0, "Bearer old");
    header(&rig.resource, 1, "Bearer old");
    header(&rig.resource, 2, "Bearer a1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_completed_driver_cleanup_cannot_discard_its_unsaved_result() {
    struct Cleanup;
    impl Cleanup {
        fn touch(&self) {}
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            panic!("exchange capture destructor");
        }
    }
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().fail_saves = vec![2, 3];
    rig.issue("a1", "r1", 0);
    let tokens = Tokens::new().with_transport(rig.tokens.clone());
    let events = rig.events.clone();
    let cleanup = Cleanup;
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), move |input: Exchange| {
        cleanup.touch();
        let tokens = tokens.clone();
        let events = events.clone();
        async move {
            events.lock().unwrap().push("exchange r0".into());
            let reply = tokens.exchange(input.refresh()).await?;
            Ok(pair(&reply.access, &reply.refresh, reply.life))
        }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Save(_)
    ));
    drop(api);
    let joined = task.await;
    assert!(
        joined.is_ok(),
        "driver output lost while destroying its completed state: {joined:?}"
    );
    let result = joined.unwrap();
    assert_eq!(&*returned_pair(&result).unwrap().refresh().expose(), "r1");
    rig.store.marker();
    rig.sent(1, 0);
    assert_eq!(rig.store.counts(), (2, 3));
    rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
}

// Faults are injected at every store/exchange boundary. Visible records and
// durability acknowledgements are deliberately separate, as with rename/fsync.
#[derive(Clone)]
struct ConservationStore {
    state: Arc<Mutex<Conservation>>,
    gate: Arc<tokio::sync::Notify>,
    drop_at: u8,
    stage: u8,
    fault: u8,
    ambiguous: bool,
    error_drops: Arc<std::sync::atomic::AtomicUsize>,
}
struct Conservation {
    record: Stored,
    durable: Option<TokenPair>,
    issued: Option<TokenPair>,
    loads: usize,
    saves: usize,
    events: Vec<String>,
}
struct CleanupBomb<F> {
    work: std::pin::Pin<Box<F>>,
    complete: bool,
    panics: bool,
    panics_pending: bool,
}
impl<F> CleanupBomb<F> {
    fn new(work: F, panics: bool) -> Self {
        Self {
            work: Box::pin(work),
            complete: false,
            panics,
            panics_pending: false,
        }
    }
    fn panic_when_pending(mut self) -> Self {
        self.panics_pending = true;
        self
    }
}
impl<F: std::future::Future> std::future::Future for CleanupBomb<F> {
    type Output = F::Output;
    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let result = self.work.as_mut().poll(cx);
        self.complete = result.is_ready();
        result
    }
}
impl<F> Drop for CleanupBomb<F> {
    fn drop(&mut self) {
        assert!(
            !((self.panics && self.complete) || (self.panics_pending && !self.complete)),
            "fault in future cleanup"
        );
    }
}
#[derive(Debug)]
struct InjectedStoreError {
    operation: &'static str,
    number: usize,
}
impl std::fmt::Display for InjectedStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("injected storage failure")
    }
}
impl std::error::Error for InjectedStoreError {}
fn storage_fault(operation: &'static str, number: usize) -> RenewalError {
    RenewalError::failed(InjectedStoreError { operation, number })
}
fn assert_store_cause(
    cause: &RenewalError,
    operation: &'static str,
    number: usize,
    dropping: bool,
) {
    assert!(matches!(cause, RenewalError::Failed(_)), "{cause:?}");
    let source = cause.source().unwrap();
    if dropping {
        assert!(source.downcast_ref::<DroppingStoreError>().is_some());
    } else {
        let fault = source.downcast_ref::<InjectedStoreError>().unwrap();
        assert_eq!((fault.operation, fault.number), (operation, number));
    }
}
fn assert_live_failure(
    error: &concord::Error,
    operation: &'static str,
    number: usize,
    fault: u8,
    dropping: bool,
) {
    let rotation = RotationError::find(error).unwrap();
    // Recovery data belongs only to the driver owner, never a middleware call.
    assert!(!matches!(rotation, RotationError::Unsaved { .. }));
    if fault == 0 || fault == 4 {
        let cause = match rotation {
            RotationError::Load(cause) if operation == "load" => cause,
            RotationError::Save(cause) if operation == "save" => cause,
            _ => panic!("unexpected live store result: {rotation:?}"),
        };
        assert_store_cause(cause, operation, number, dropping);
    } else {
        assert!(matches!(rotation, RotationError::Stopped));
    }
}
impl CredentialStore for ConservationStore {
    fn load(&self) -> impl std::future::Future<Output = Result<Stored, RenewalError>> + Send {
        let number = {
            let mut state = self.state.lock().unwrap();
            state.loads += 1;
            state.events.push("load".into());
            state.loads
        };
        let fails = (number == 1 && self.stage == 0) || (number == 2 && self.stage == 4);
        assert!(!(fails && self.fault == 3), "load factory panic");
        CleanupBomb::new(
            async move {
                if number == 1 && self.drop_at == 0 {
                    self.gate.notified().await;
                }
                assert!(!(fails && self.fault == 1), "load poll panic");
                if fails && (self.fault == 0 || self.fault == 4) {
                    return Err(if self.fault == 4 && number == 1 {
                        RenewalError::failed(DroppingStoreError {
                            drops: self.error_drops.clone(),
                        })
                    } else {
                        storage_fault("load", number)
                    });
                }
                Ok(self.state.lock().unwrap().record.clone())
            },
            fails && self.fault == 2,
        )
    }
    fn save(
        &self,
        record: Stored,
    ) -> impl std::future::Future<Output = Result<(), RenewalError>> + Send {
        let number = {
            let mut state = self.state.lock().unwrap();
            state.saves += 1;
            state.events.push(match &record {
                Stored::Refreshing => "mark".into(),
                Stored::Ready(_) => "save r1".into(),
            });
            state.saves
        };
        let fails = (number == 1 && self.stage == 1)
            || (number >= 2 && self.stage == 3)
            || (number == 3 && self.stage == 5);
        let force_retry = number == 2 && self.stage >= 4;
        assert!(!(fails && self.fault == 3), "save factory panic");
        CleanupBomb::new(
            async move {
                if (number == 1 && self.drop_at == 1) || (number == 2 && self.drop_at == 3) {
                    self.gate.notified().await;
                }
                assert!(!(fails && self.fault == 1), "save poll panic");
                let error = force_retry || (fails && (self.fault == 0 || self.fault == 4));
                let mut state = self.state.lock().unwrap();
                if !error || self.ambiguous {
                    state.record = record.clone();
                }
                if !error {
                    state.durable = match record {
                        Stored::Ready(pair) => Some(pair),
                        Stored::Refreshing => None,
                    };
                }
                drop(state);
                if error {
                    Err(if self.fault == 4 && (number == 1 || number == 2) {
                        RenewalError::failed(DroppingStoreError {
                            drops: self.error_drops.clone(),
                        })
                    } else {
                        storage_fault("save", number)
                    })
                } else {
                    Ok(())
                }
            },
            fails && self.fault == 2,
        )
    }
}
async fn conserve_pair(
    stage: u8,
    fault: u8,
    drop_at: u8,
    ambiguous: bool,
    unusable: bool,
    live: bool,
) {
    let state = Arc::new(Mutex::new(Conservation {
        record: expired(),
        durable: None,
        issued: None,
        loads: 0,
        saves: 0,
        events: Vec::new(),
    }));
    let gate = Arc::new(tokio::sync::Notify::new());
    let error_drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let store = ConservationStore {
        error_drops: error_drops.clone(),
        state: state.clone(),
        gate: gate.clone(),
        drop_at,
        stage,
        fault,
        ambiguous,
    };
    let script = Script::new();
    if stage == 2 && fault == 0 {
        script.answer(500).read_upload();
    } else {
        script
            .answer(200)
            .read_upload()
            .body(r#"{"access":"a1","refresh":"r1","life":100}"#);
    }
    let tokens = Tokens::new().with_transport(script.clone());
    let exchange_state = state.clone();
    let exchange_gate = gate.clone();
    let (auth, driver) = Rotating::new(&base(), store, move |input: Exchange| {
        exchange_state
            .lock()
            .unwrap()
            .events
            .push("exchange r0".into());
        assert!(!(stage == 2 && fault == 3), "exchange factory panic");
        let state = exchange_state.clone();
        let gate = exchange_gate.clone();
        let tokens = tokens.clone();
        CleanupBomb::new(
            async move {
                if drop_at == 2 {
                    gate.notified().await;
                }
                assert!(!(stage == 2 && fault == 1), "exchange poll panic");
                let reply = tokens.exchange(input.refresh()).await?;
                let pair = TokenPair::new(
                    reply.access,
                    reply.refresh,
                    if unusable {
                        None
                    } else {
                        Some(SystemTime::now() + sec(reply.life))
                    },
                );
                state.lock().unwrap().issued = Some(pair.clone());
                Ok(pair)
            },
            stage == 2 && fault == 2,
        )
    });
    let resources = Script::new();
    resources.answer(200).body("ok");
    let api = Data::new()
        .with_transport(resources.clone())
        .with_middleware(auth);
    let task = tokio::spawn(async move {
        let mut driver = driver;
        let result = (&mut driver).await;
        assert!(
            matches!((&mut driver).await, Err(RotationError::Stopped)),
            "a completed driver must never hand the pair out twice"
        );
        result
    });
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    if live {
        gate.notify_one();
        let outcome = call.await;
        match (stage, fault) {
            (0, _) => {
                assert_live_failure(outcome.as_ref().unwrap_err(), "load", 1, fault, fault == 4)
            }
            (1, _) => {
                assert_live_failure(outcome.as_ref().unwrap_err(), "save", 1, fault, fault == 4)
            }
            (2, 0) => {
                let RotationError::Renewal(cause) = cause(outcome.as_ref().unwrap_err()) else {
                    panic!("exchange failure expected");
                };
                assert!(matches!(cause, RenewalError::Failed(_)));
                assert_eq!(
                    cause
                        .source()
                        .unwrap()
                        .downcast_ref::<concord::Error>()
                        .unwrap()
                        .status()
                        .unwrap()
                        .as_u16(),
                    500
                );
            }
            (2, _) => assert_renewal(outcome.as_ref().unwrap_err(), |cause| {
                if fault == 2 && unusable {
                    matches!(cause, RenewalError::Expired)
                } else {
                    matches!(cause, RenewalError::Panicked)
                }
            }),
            (3, _) => {
                assert_live_failure(outcome.as_ref().unwrap_err(), "save", 2, fault, fault == 4)
            }
            _ => assert_live_failure(outcome.as_ref().unwrap_err(), "save", 2, 0, fault == 4),
        }
        // Release the call's copy of an author error before the next hand-over.
        drop(outcome);
        if stage >= 3 && (stage >= 4 || fault == 0 || fault == 4) {
            let again = api.get().await.unwrap_err();
            if stage == 4 {
                assert_live_failure(&again, "load", 2, fault, false);
            } else {
                assert_live_failure(&again, "save", 3, fault, false);
            }
            drop(again);
        }
    } else {
        drop(call);
    }
    drop(api);
    gate.notify_one();
    let result = task.await.unwrap();
    let state = state.lock().unwrap();
    if let Some(issued) = &state.issued {
        let snapshot = |pair: &TokenPair| {
            (
                pair.access().expose().to_string(),
                pair.refresh().expose().to_string(),
                pair.expires_at(),
            )
        };
        let handed_back = returned_pair(&result);
        if let Some(returned) = &handed_back {
            assert_eq!(snapshot(returned), snapshot(issued));
            let error = result.as_ref().unwrap_err();
            assert_eq!(format!("{error:?}"), "RotationError::Unsaved");
            let RotationError::Unsaved { cause, .. } = error else {
                unreachable!()
            };
            if fault == 0 {
                let (operation, number) = if stage == 4 {
                    ("load", 2)
                } else {
                    ("save", if live && stage == 3 { 4 } else { 3 })
                };
                assert_store_cause(cause, operation, number, false);
            } else {
                assert!(matches!(cause, RenewalError::Panicked), "{cause:?}");
            }
            assert!(!error.to_string().contains("r1"));
        }
        assert!(state.durable.as_ref().is_some_and(|pair| snapshot(pair) == snapshot(issued)) || handed_back.is_some(),
            "pair lost: stage={stage}, fault={fault}, drop={drop_at}, visible={ambiguous}, result={result:?}, events={:?}", state.events);
    }
    // Pin every operation, including the final recovery attempt: no hidden send.
    let expected: &[&str] = if (!live && drop_at == 0) || stage == 0 {
        &["load"]
    } else if stage == 1 {
        &["load", "mark"]
    } else if stage == 2 && fault != 2 {
        &["load", "mark", "exchange r0"]
    } else if live && fault == 0 && stage >= 3 {
        &[
            "load",
            "mark",
            "exchange r0",
            "save r1",
            "load",
            "save r1",
            "load",
            "save r1",
        ]
    } else if (stage == 3 && (fault == 0 || fault == 4)) || stage >= 4 {
        &["load", "mark", "exchange r0", "save r1", "load", "save r1"]
    } else {
        &["load", "mark", "exchange r0", "save r1"]
    };
    let expected: Vec<_> = expected
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, event)| {
            // A failed recovery load has no corresponding save in that attempt.
            if stage == 4 && index == 5 {
                None
            } else {
                Some(event)
            }
        })
        .collect();
    assert_eq!(
        state.events, expected,
        "stage={stage}, fault={fault}, drop={drop_at}"
    );
    assert_eq!(
        state.loads,
        expected.iter().filter(|event| **event == "load").count()
    );
    assert_eq!(
        state.saves,
        expected
            .iter()
            .filter(|event| **event == "mark" || **event == "save r1")
            .count()
    );
    let sends = usize::from(
        (live || drop_at != 0) && stage >= 2 && !(stage == 2 && (fault == 1 || fault == 3)),
    );
    assert_eq!(script.requests().len(), sends);
    if sends == 1 {
        assert_eq!(script.requests()[0].upload().bytes(), b"refresh_token=r0");
        assert_eq!(script.requests()[0].info().number(), 1);
    }
    assert_eq!(resources.requests().len(), 0);
    drop(state);
    // An author error returned to the test may itself panic on final release.
    let cleanup = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(result)));
    assert_eq!(
        cleanup.is_err(),
        live && fault == 4 && stage <= 1 || !live && drop_at != 0 && fault == 4 && stage <= 1
    );
    assert_eq!(
        error_drops.load(std::sync::atomic::Ordering::SeqCst),
        usize::from(fault == 4 && (live || drop_at != 0))
    );
}
proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config { cases: 16, failure_persistence: None, ..proptest::test_runner::Config::default() })]
    #[test]
    fn c12_6_issued_pair_is_durable_or_returned_at_every_fault_boundary(ambiguous in proptest::bool::ANY, unusable in proptest::bool::ANY) {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_time().build().unwrap();
        runtime.block_on(async {
            tokio::time::pause();
            for live in [false, true] { for drop_at in 0..4 { for stage in 0..6 { for fault in 0..5 {
                if stage == 2 && fault == 4 { continue; } // The extra error belongs to the store.
                conserve_pair(stage, fault, drop_at, ambiguous, unusable, live).await;
            }}}}
        });
    }
}

#[derive(Debug)]
struct DroppingStoreError {
    drops: Arc<std::sync::atomic::AtomicUsize>,
}
impl std::fmt::Display for DroppingStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("injected store error")
    }
}
impl std::error::Error for DroppingStoreError {}
impl Drop for DroppingStoreError {
    fn drop(&mut self) {
        self.drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        panic!("store error destructor");
    }
}
#[derive(Clone)]
struct HandoverStore {
    store: MemoryStore,
    drops: Arc<std::sync::atomic::AtomicUsize>,
}
impl CredentialStore for HandoverStore {
    async fn load(&self) -> Result<Stored, RenewalError> {
        self.store.load().await
    }
    async fn save(&self, record: Stored) -> Result<(), RenewalError> {
        let result = self.store.save(record).await;
        if self.store.counts().1 == 2 {
            assert!(result.is_err());
            Err(RenewalError::failed(DroppingStoreError {
                drops: self.drops.clone(),
            }))
        } else {
            result
        }
    }
}
#[tokio::test(start_paused = true)]
async fn c12_6_unsaved_pair_survives_previous_store_error_drop() {
    for recovery_panics in [false, true] {
        let rig = Rig::new(expired());
        {
            let mut state = rig.store.0.lock().unwrap();
            state.fail_saves = vec![2, 3];
            state.panic_save = recovery_panics.then_some(3);
        }
        rig.issue("a1", "r1", 0);
        let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (auth, driver) = auth(
            HandoverStore {
                store: rig.store.clone(),
                drops: drops.clone(),
            },
            rig.tokens.clone(),
            rig.events.clone(),
        );
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::Save(_)
        ));
        drop(api);
        let result = task.await.unwrap();
        assert_eq!(
            returned_pair(&result)
                .map(|pair| pair.refresh().expose().to_string())
                .as_deref(),
            Some("r1"),
            "replacement lost while dropping previous error: {result:?}"
        );
        assert!(matches!(
            result,
            Err(RotationError::Unsaved {
                cause: RenewalError::Panicked,
                ..
            })
        ));
        assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
        rig.store.marker();
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (2, 3));
        rig.events(&["load", "mark", "exchange r0", "save r1", "load", "save r1"]);
    }
}

#[tokio::test(start_paused = true)]
async fn c12_7_cancelled_initial_load_cleanup_panic_stops_the_credential() {
    struct PendingCleanupStore(MemoryStore);
    impl CredentialStore for PendingCleanupStore {
        fn load(&self) -> impl std::future::Future<Output = Result<Stored, RenewalError>> + Send {
            CleanupBomb::new(self.0.load(), false).panic_when_pending()
        }
        async fn save(&self, record: Stored) -> Result<(), RenewalError> {
            self.0.save(record).await
        }
    }
    let rig = Rig::new(expired());
    rig.store.0.lock().unwrap().load_delay = sec(1000);
    let (auth, driver) = auth(
        PendingCleanupStore(rig.store.clone()),
        rig.tokens.clone(),
        rig.events.clone(),
    );
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    assert_eq!(rig.store.counts(), (1, 0));
    drop(call);
    ticks().await;
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::Stopped
    ));
    drop(api);
    assert!(matches!(task.await.unwrap(), Err(RotationError::Stopped)));
    rig.sent(0, 0);
    rig.events(&["load"]);
    assert_eq!(rig.store.counts(), (1, 0));
}

// An exchange that states that its request never left.

fn auth_mapped(rig: &Rig, map: fn(concord::Error) -> RenewalError) -> (Rotating, RotationDriver) {
    let tokens = Tokens::new()
        .with_transport(rig.tokens.clone())
        .with_idle_timeout(sec(3600));
    let events = rig.events.clone();
    Rotating::new(&base(), rig.store.clone(), move |exchange: Exchange| {
        let tokens = tokens.clone();
        let events = events.clone();
        async move {
            events
                .lock()
                .unwrap()
                .push(format!("exchange {}", &*exchange.refresh().expose()));
            let reply = tokens.exchange(exchange.refresh()).await.map_err(map)?;
            Ok(TokenPair::expires_in(
                reply.access,
                reply.refresh,
                sec(reply.life),
            ))
        }
    })
}
fn refuse(rig: &Rig, delay: u64) {
    rig.tokens
        .fail(concord::transport::TransportError::blocked())
        .after(sec(delay));
}
fn not_sent(error: &concord::Error) -> bool {
    match cause(error) {
        RotationError::Renewal(RenewalError::NotSent(cause)) => cause
            .downcast_ref::<concord::Error>()
            .is_some_and(|endpoint| endpoint.class() == Some(concord::error::Class::Blocked)),
        _ => false,
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_exchange_not_sent_saves_the_pair_again_and_the_next_call_exchanges_it() {
    let rig = Rig::new(Stored::Ready(TokenPair::new("a0", "r0", None)));
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);

    assert!(not_sent(&api.get().await.unwrap_err()));
    rig.store.ready("a0", "r0");
    rig.sent(1, 0);
    assert_eq!(api.get().await.unwrap(), "ok");
    rig.store.ready("a1", "r1");
    rig.sent(2, 1);
    header(&rig.resource, 0, "Bearer a1");
    // The states of the store, in order: the marker, the same pair again, the
    // marker of the second exchange, its replacement.
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r0",
        "mark",
        "exchange r0",
        "save r1",
    ]);
    assert_eq!(rig.store.counts(), (1, 4));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_refused_save_of_the_previous_pair_keeps_the_marker() {
    for ambiguous in [false, true] {
        let rig = Rig::new(expired());
        {
            let mut s = rig.store.0.lock().unwrap();
            s.fail_save = Some(2);
            s.ambiguous = ambiguous;
        }
        refuse(&rig, 0);
        rig.issue("a1", "r1", 0);
        let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        assert!(not_sent(&api.get().await.unwrap_err()));
        assert!(matches!(
            cause(&api.get().await.unwrap_err()),
            RotationError::NeedsReauthorization
        ));
        if ambiguous {
            // The failed save took effect; the pair it wrote is still unused.
            rig.store.ready("old", "r0");
        } else {
            rig.store.marker();
        }
        rig.sent(1, 0);
        assert_eq!(rig.store.counts(), (1, 2));
        rig.events(&["load", "mark", "exchange r0", "save r0"]);
        drop(api);
        assert!(task.await.unwrap().is_err());
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_calls_waiting_on_an_exchange_not_sent_fail_without_being_sent() {
    let rig = Rig::new(expired());
    refuse(&rig, 3);
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    for result in join_all((0..8).map(|_| api.get().send())).await {
        assert!(not_sent(&result.unwrap_err()));
    }
    rig.sent(1, 0);
    rig.store.ready("old", "r0");
    api.get().await.unwrap();
    rig.sent(2, 1);
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r0",
        "mark",
        "exchange r0",
        "save r1",
    ]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_two_exchanges_not_sent_in_a_row_keep_the_same_refresh_token() {
    let rig = Rig::new(expired());
    refuse(&rig, 0);
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(not_sent(&api.get().await.unwrap_err()));
    assert!(not_sent(&api.get().await.unwrap_err()));
    rig.store.ready("old", "r0");
    api.get().await.unwrap();
    rig.sent(3, 1);
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r0",
        "mark",
        "exchange r0",
        "save r0",
        "mark",
        "exchange r0",
        "save r1",
    ]);
    assert_eq!(rig.store.counts(), (1, 6));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_dropped_call_does_not_cancel_the_save_of_the_previous_pair() {
    let rig = Rig::new(expired());
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.store.0.lock().unwrap().gate_save = Some((2, gate.clone()));
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).body("ok");
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    ticks().await;
    rig.events(&["load", "mark", "exchange r0", "save r0"]);
    rig.store.marker();
    drop(call);
    ticks().await;
    rig.store.marker();
    gate.notify_one();
    ticks().await;
    rig.store.ready("old", "r0");
    api.get().await.unwrap();
    rig.sent(2, 1);
    rig.store.ready("a1", "r1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_7_a_driver_dropped_during_the_save_of_the_previous_pair_leaves_the_marker() {
    let rig = Rig::new(expired());
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.store.0.lock().unwrap().gate_save = Some((2, gate));
    refuse(&rig, 0);
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let mut driver = Box::pin(driver);
    let mut call = Box::pin(api.get().send());
    assert!(call.as_mut().now_or_never().is_none());
    for _ in 0..16 {
        assert!(driver.as_mut().now_or_never().is_none());
    }
    rig.events(&["load", "mark", "exchange r0", "save r0"]);
    drop(driver);
    assert!(matches!(
        cause(&call.await.unwrap_err()),
        RotationError::Stopped
    ));
    rig.store.marker();
    // A later owner of the account finds the marker: the safe side.
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    rig.sent(1, 0);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_6_only_a_refusal_before_the_first_attempt_is_not_sent() {
    use concord::transport::TransportError;
    for (failure, sent) in [
        (TransportError::blocked(), true),
        (TransportError::connect(), false),
        (TransportError::timeout(), false),
    ] {
        let rig = Rig::new(expired());
        rig.tokens.fail(failure);
        rig.issue("a1", "r1", 0);
        rig.resource.answer(200).body("ok");
        let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
        let api = rig.client(auth);
        let task = tokio::spawn(driver);
        let error = api.get().await.unwrap_err();
        assert_eq!(not_sent(&error), sent);
        let next = api.get().await;
        if sent {
            assert_eq!(next.unwrap(), "ok");
            rig.store.ready("a1", "r1");
            rig.sent(2, 1);
            drop(api);
            task.await.unwrap().unwrap();
            continue;
        } else {
            assert_renewal(&error, |e| matches!(e, RenewalError::Failed(_)));
            rig.store.marker();
            assert!(matches!(
                cause(&next.unwrap_err()),
                RotationError::NeedsReauthorization
            ));
            rig.sent(1, 0);
        }
        drop(api);
        assert!(task.await.unwrap().is_err());
    }

    // A refusal on a second attempt comes after a first one that may have been sent.
    let tokens = Script::new();
    tokens.answer(503);
    tokens.fail(TransportError::blocked());
    let client = Tokens::new().with_transport(tokens.clone()).with_retry(
        Retry::none()
            .tries(2)
            .not_applied_on(503)
            .when(|_: &concord::retry::Tried<'_, ()>| Some(Duration::ZERO)),
    );
    let Err(error) = client.exchange(Secret::new("r0")).await else {
        panic!("the second attempt is refused");
    };
    assert_eq!(error.tries(), 2);
    assert!(matches!(
        RenewalError::from_call(error),
        RenewalError::Failed(_)
    ));

    let error = RenewalError::not_sent(std::io::Error::other("private cause"));
    assert_eq!(format!("{error:?}"), "RenewalError::NotSent");
    assert_eq!(
        error.to_string(),
        "the credential could not be renewed, and its request was not sent"
    );
    assert!(error
        .source()
        .is_some_and(|s| s.to_string() == "private cause"));
}

// A credential that the API rejects inside a `200`.

#[derive(Debug, Deserialize)]
struct SlackProblem {
    error: String,
}
fn slack_envelope(body: serde_json::Value) -> Result<String, concord::Fail<SlackProblem>> {
    if body.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        return Ok(body["user"].as_str().unwrap_or_default().to_owned());
    }
    let problem: SlackProblem =
        serde_json::from_value(body).map_err(|_| concord::mismatch("ok: false with no error"))?;
    Err(problem)?
}
concord::api! {
    client Slack {
        base "https://slack.test/api";
        read concord::Json;
        problem SlackProblem;
        status { 2xx { body } => slack_envelope(body)? }
        POST auth_test() -> String { path "auth.test"; }
    }
}
// What the client's author states once: the errors of the envelope that reject the token.
fn slack_rejects(seen: &concord::auth::Seen<'_>) -> bool {
    #[derive(Deserialize)]
    struct Envelope {
        ok: bool,
        error: Option<String>,
    }
    seen.status() == 200
        && serde_json::from_slice::<Envelope>(seen.body()).is_ok_and(|reply| {
            !reply.ok
                && matches!(
                    reply.error.as_deref(),
                    Some("token_expired" | "token_revoked" | "invalid_auth")
                )
        })
}
fn rejected(seen: &concord::auth::Seen<'_>) -> bool {
    seen.body() == b"rejected"
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_token_expired_inside_a_200_makes_the_next_call_exchange() {
    let future = SystemTime::now() + sec(3600);
    let rig = Rig::new(Stored::Ready(TokenPair::new(
        "xoxe.xoxb-a",
        "xoxe-1-r",
        Some(future),
    )));
    rig.issue("xoxe.xoxb-b", "xoxe-1-s", 0);
    rig.resource
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"ok": false, "error": "token_expired"}"#);
    rig.resource
        .answer(200)
        .header("content-type", "application/json")
        .body(r#"{"ok": true, "user": "U1"}"#);
    let tokens = Tokens::new().with_transport(rig.tokens.clone());
    let events = rig.events.clone();
    let (auth, driver) = Rotating::new(
        &"https://slack.test".parse().unwrap(),
        rig.store.clone(),
        move |exchange: Exchange| {
            let tokens = tokens.clone();
            let events = events.clone();
            async move {
                events
                    .lock()
                    .unwrap()
                    .push(format!("exchange {}", &*exchange.refresh().expose()));
                let reply = tokens.exchange(exchange.refresh()).await?;
                Ok(TokenPair::expires_in(
                    reply.access,
                    reply.refresh,
                    sec(reply.life),
                ))
            }
        },
    );
    let task = tokio::spawn(driver);
    let slack = Slack::new()
        .with_transport(rig.resource.clone())
        .with_middleware(auth.rejected_when(slack_rejects))
        .with_retry(Retry::standard().not_applied_on(429).when(after_401));

    let first = slack.auth_test().await.unwrap_err();
    assert_eq!(first.problem().unwrap().error, "token_expired");
    assert_eq!(first.tries(), 1);
    rig.sent(0, 1);
    assert_eq!(slack.auth_test().await.unwrap(), "U1");
    rig.sent(1, 2);
    header(&rig.resource, 0, "Bearer xoxe.xoxb-a");
    header(&rig.resource, 1, "Bearer xoxe.xoxb-b");
    assert_eq!(
        rig.tokens.requests()[0].upload().bytes(),
        b"refresh_token=xoxe-1-r"
    );
    rig.events(&["load", "mark", "exchange xoxe-1-r", "save xoxe-1-s"]);
    rig.store.ready("xoxe.xoxb-b", "xoxe-1-s");
    drop(slack);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_late_rejection_inside_a_200_keeps_the_newer_pair() {
    let rig = Rig::new(Stored::Ready(pair("a0", "r0", 100)));
    rig.issue("a1", "r1", 0);
    rig.resource.answer(200).after(sec(5)).body("rejected");
    rig.resource.answer(200).body("rejected");
    rig.resource.answer(200).body("c");
    rig.resource.answer(200).body("d");
    let (auth, driver) = rig.auth();
    let api = rig
        .client(auth.rejected_when(rejected))
        .with_retry(Retry::none().tries(2).when(after_401));
    let task = tokio::spawn(driver);
    let late = tokio::spawn({
        let api = api.clone();
        async move { api.get().await }
    });
    ticks().await;
    rig.sent(0, 1);
    assert_eq!(api.get().await.unwrap(), "rejected");
    assert_eq!(api.get().await.unwrap(), "c");
    assert_eq!(late.await.unwrap().unwrap(), "rejected");
    assert_eq!(api.get().await.unwrap(), "d");
    rig.sent(1, 4);
    header(&rig.resource, 0, "Bearer a0");
    header(&rig.resource, 1, "Bearer a0");
    header(&rig.resource, 2, "Bearer a1");
    header(&rig.resource, 3, "Bearer a1");
    rig.events(&["load", "mark", "exchange r0", "save r1"]);
    assert!(rig
        .resource
        .requests()
        .iter()
        .all(|request| request.info().number() == 1));
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_rejection_inside_a_200_is_shown_only_a_whole_body_of_at_most_64_kib() {
    let large = "x".repeat(64 * 1024 + 1);
    let half = "z".repeat(40 * 1024);
    let full = "y".repeat(64 * 1024);
    let rig = Rig::new(Stored::Ready(pair("a0", "r0", 100)));
    rig.issue("a1", "r1", 0);
    rig.resource
        .answer(200)
        .body(half.clone())
        .body(half.clone());
    rig.resource.answer(200).body(large.clone());
    rig.resource
        .answer(200)
        .declare_length(u64::try_from(large.len()).unwrap())
        .body(large.clone());
    rig.resource.answer(200).body("panic");
    rig.resource.answer(200).body(full.clone());
    rig.resource.answer(200).body("after");
    let shown = Arc::new(Mutex::new(Vec::new()));
    let (auth, driver) = rig.auth();
    let auth = auth.rejected_when({
        let shown = shown.clone();
        move |seen| {
            assert_eq!(format!("{seen:?}"), "Seen(..)");
            shown.lock().unwrap().push(seen.body().len());
            assert!(seen.body() != b"panic", "private panic");
            seen.body().len() == 64 * 1024
        }
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_eq!(api.get().await.unwrap(), half.repeat(2));
    assert_eq!(api.get().await.unwrap(), large);
    assert_eq!(api.get().await.unwrap(), large);
    assert_eq!(api.get().await.unwrap(), "panic");
    assert_eq!(api.get().await.unwrap(), full);
    assert_eq!(api.get().await.unwrap(), "after");
    // No long body is shown, whether its length is known or not; a panic rejects
    // nothing; a body of exactly 64 KiB is shown and rejects the token.
    assert_eq!(*shown.lock().unwrap(), [5, 64 * 1024, 5]);
    rig.sent(1, 6);
    header(&rig.resource, 4, "Bearer a0");
    header(&rig.resource, 5, "Bearer a1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_body_that_is_not_read_to_its_end_rejects_nothing() {
    let rig = Rig::new(Stored::Ready(pair("a0", "r0", 100)));
    rig.resource.answer(200).body("rejected").stall();
    rig.resource.answer(200).body("ok");
    let (auth, driver) = rig.auth();
    let api = rig.client(auth.rejected_when(rejected));
    let task = tokio::spawn(driver);
    assert!(api
        .get()
        .with_timeout(sec(1))
        .await
        .unwrap_err()
        .is_timeout());
    assert_eq!(api.get().await.unwrap(), "ok");
    rig.sent(0, 2);
    header(&rig.resource, 1, "Bearer a0");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_body_of_unknown_length_is_shown_only_up_to_64_kib() {
    let rig = Rig::new(Stored::Ready(pair("a0", "r0", 100)));
    rig.issue("a1", "r1", 0);
    let sizes = Arc::new(Mutex::new(vec![40 * 1024, 40 * 1024, 30 * 1024, 30 * 1024]));
    let carried = Arc::new(Mutex::new(Vec::new()));
    let transport = concord::fake({
        let carried = carried.clone();
        move |request| {
            carried.lock().unwrap().push(
                request.headers()["authorization"]
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
            let mut sizes = sizes.lock().unwrap();
            let count = sizes.len().min(2);
            let pieces: Vec<_> = sizes
                .drain(..count)
                .map(|size| Ok::<_, std::io::Error>(concord::bytes::Bytes::from(vec![b'z'; size])))
                .collect();
            Ok(concord::reply(
                200,
                concord::reqwest::Body::wrap_stream(futures_util::stream::iter(pieces)),
            ))
        }
    });
    let shown = Arc::new(Mutex::new(Vec::new()));
    let (auth, driver) = rig.auth();
    let auth = auth.rejected_when({
        let shown = shown.clone();
        move |seen| {
            shown.lock().unwrap().push(seen.body().len());
            true
        }
    });
    let api = Data::new().with_transport(transport).with_middleware(auth);
    let task = tokio::spawn(driver);
    assert_eq!(api.get().await.unwrap().len(), 80 * 1024);
    assert_eq!(api.get().await.unwrap().len(), 60 * 1024);
    assert_eq!(api.get().await.unwrap().len(), 0);
    assert_eq!(*shown.lock().unwrap(), [60 * 1024, 0]);
    assert_eq!(
        *carried.lock().unwrap(),
        ["Bearer a0", "Bearer a0", "Bearer a1"]
    );
    rig.sent(1, 0);
    drop(api);
    task.await.unwrap().unwrap();
}

// A restored pair is never sent again as an access token.

fn stored_expiry(store: &MemoryStore) -> Option<SystemTime> {
    match &store.0.lock().unwrap().record {
        Stored::Ready(pair) => pair.expires_at(),
        Stored::Refreshing => panic!("expected a saved pair"),
    }
}

#[tokio::test(start_paused = true)]
async fn c12_6_after_a_restore_the_next_call_exchanges_even_after_a_late_401() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
    rig.resource.answer(401).after(sec(5));
    rig.resource.answer(200).body("after restore");
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth.clone());
    let task = tokio::spawn(driver);
    let delayed = tokio::spawn({
        let api = api.clone();
        async move { api.get().await }
    });
    ticks().await;
    rig.sent(0, 1);
    // Renew early, so that the next call exchanges while the first one waits.
    drop(auth.clone().early_by(sec(101)));
    let gate = Arc::new(tokio::sync::Notify::new());
    rig.store.0.lock().unwrap().gate_save = Some((2, gate.clone()));
    let waiting = tokio::spawn({
        let api = api.clone();
        async move { api.get().await }
    });
    ticks().await;
    rig.store.marker();
    // The 401 arrives while the previous pair is being saved again.
    assert!(delayed.await.unwrap().unwrap_err().is_status(401));
    drop(auth.early_by(sec(0)));
    gate.notify_one();
    assert!(not_sent(&waiting.await.unwrap().unwrap_err()));
    rig.store.ready("old", "r0");
    assert_eq!(stored_expiry(&rig.store), None);
    assert_eq!(api.get().await.unwrap(), "after restore");
    header(&rig.resource, 1, "Bearer a1");
    rig.sent(2, 2);
    rig.events(&[
        "load",
        "mark",
        "exchange r0",
        "save r0",
        "mark",
        "exchange r0",
        "save r1",
    ]);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_an_owner_that_loads_a_restored_pair_exchanges_it() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 3600)));
    rig.resource.answer(401);
    rig.resource.answer(200).body("restart");
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(api.get().await.unwrap_err().is_status(401));
    assert!(not_sent(&api.get().await.unwrap_err()));
    rig.store.ready("old", "r0");
    assert_eq!(stored_expiry(&rig.store), None);
    drop(api);
    assert!(task.await.unwrap().is_err());

    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_eq!(api.get().await.unwrap(), "restart");
    header(&rig.resource, 1, "Bearer a1");
    rig.sent(2, 2);
    rig.store.ready("a1", "r1");
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_panic_dropping_an_exchange_not_sent_reports_the_panic_and_keeps_the_block() {
    let rig = Rig::new(expired());
    let (auth, driver) = Rotating::new(&base(), rig.store.clone(), |_| CompletedExchangeDrop {
        complete: false,
        work: Box::pin(async { Err(RenewalError::not_sent(std::io::Error::other("offline"))) }),
    });
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_renewal(&api.get().await.unwrap_err(), |e| {
        matches!(e, RenewalError::Panicked)
    });
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    rig.store.marker();
    rig.events(&["load", "mark"]);
    drop(api);
    assert!(task.await.unwrap().is_err());
}

#[tokio::test(start_paused = true)]
async fn c12_6_a_restore_that_wrote_before_failing_leaves_a_pair_the_next_owner_exchanges() {
    let rig = Rig::new(Stored::Ready(pair("old", "r0", 3600)));
    {
        let mut s = rig.store.0.lock().unwrap();
        s.fail_save = Some(2);
        s.ambiguous = true;
    }
    rig.resource.answer(401);
    rig.resource.answer(200).body("ok");
    refuse(&rig, 0);
    rig.issue("a1", "r1", 0);
    let (auth, driver) = auth_mapped(&rig, RenewalError::from_call);
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert!(api.get().await.unwrap_err().is_status(401));
    assert!(not_sent(&api.get().await.unwrap_err()));
    // This instance stays blocked; the store holds the pair, without a usable expiry.
    assert!(matches!(
        cause(&api.get().await.unwrap_err()),
        RotationError::NeedsReauthorization
    ));
    rig.store.ready("old", "r0");
    assert_eq!(stored_expiry(&rig.store), None);
    drop(api);
    assert!(task.await.unwrap().is_err());

    let (auth, driver) = rig.auth();
    let api = rig.client(auth);
    let task = tokio::spawn(driver);
    assert_eq!(api.get().await.unwrap(), "ok");
    header(&rig.resource, 1, "Bearer a1");
    rig.sent(2, 2);
    drop(api);
    task.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_late_rejection_keeps_the_newer_token_while_an_older_request_is_alive() {
    for rotating in [false, true] {
        let rig = Rig::new(Stored::Ready(pair("old", "r0", 100)));
        rig.issue("replacement", "r1", 0);
        let mut task = None;
        let renewals = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let api = if rotating {
            let (auth, driver) = rig.auth();
            task = Some(tokio::spawn(driver));
            rig.client(auth.rejected_when(rejected))
        } else {
            let auth = concord::auth::Renewed::new(&base(), {
                let renewals = renewals.clone();
                move || {
                    renewals.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    async { concord::auth::AccessToken::expires_in("replacement", sec(200)) }
                }
            })
            .with_token(concord::auth::AccessToken::expires_in("old", sec(200)).unwrap())
            .unwrap();
            Data::new()
                .with_transport(rig.resource.clone())
                .with_middleware(auth.rejected_when(rejected))
        };
        rig.resource.answer(200).after(sec(20)).body("holder");
        rig.resource.answer(200).after(sec(2)).body("rejected");
        rig.resource.answer(200).body("rejected");
        rig.resource.answer(200).body("ok");
        rig.resource.answer(200).body("still ok");
        // The holder keeps the old generation alive across the whole sequence.
        let holder = tokio::spawn({
            let api = api.clone();
            async move { api.get().await }
        });
        ticks().await;
        let late = tokio::spawn({
            let api = api.clone();
            async move { api.get().await }
        });
        ticks().await;
        assert_eq!(api.get().await.unwrap(), "rejected");
        assert_eq!(api.get().await.unwrap(), "ok");
        assert_eq!(late.await.unwrap().unwrap(), "rejected");
        assert_eq!(api.get().await.unwrap(), "still ok");
        header(&rig.resource, 3, "Bearer replacement");
        header(&rig.resource, 4, "Bearer replacement");
        assert!(!holder.is_finished());
        holder.abort();
        let _ = holder.await;
        drop(api);
        if let Some(task) = task {
            task.await.unwrap().unwrap();
            rig.sent(1, 5);
        } else {
            assert_eq!(renewals.load(std::sync::atomic::Ordering::SeqCst), 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn c12_8_a_401_from_another_origin_rejects_nothing() {
    use concord::reqwest::ResponseBuilderExt;
    for rotating in [true, false] {
        let rig = Rig::new(Stored::Ready(pair("old", "r0", 200)));
        rig.issue("new", "r1", 0);
        let carried = Arc::new(Mutex::new(Vec::new()));
        let transport = concord::fake({
            let carried = carried.clone();
            move |request| {
                let mut carried = carried.lock().unwrap();
                carried.push(
                    request.headers()["authorization"]
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
                // A transport that followed a redirect to another host reports it.
                Ok(if carried.len() == 1 {
                    concord::http::Response::builder()
                        .status(401)
                        .url("https://foreign.test/redirected".parse().unwrap())
                        .body("")
                        .unwrap()
                        .into()
                } else {
                    concord::reply(200, "ok")
                })
            }
        });
        let mut task = None;
        let api = if rotating {
            let (auth, driver) = rig.auth();
            task = Some(tokio::spawn(driver));
            Data::new().with_transport(transport).with_middleware(auth)
        } else {
            let auth = concord::auth::Renewed::new(&base(), || async {
                concord::auth::AccessToken::expires_in("replacement", sec(200))
            })
            .with_token(concord::auth::AccessToken::expires_in("old", sec(200)).unwrap())
            .unwrap();
            Data::new().with_transport(transport).with_middleware(auth)
        };
        assert!(api.get().await.unwrap_err().is_status(401));
        assert_eq!(api.get().await.unwrap(), "ok");
        assert_eq!(*carried.lock().unwrap(), ["Bearer old", "Bearer old"]);
        drop(api);
        if let Some(task) = task {
            task.await.unwrap().unwrap();
            rig.sent(0, 0);
            rig.store.ready("old", "r0");
        }
    }
}
