//! Value semantics: required, optional and defaulted values, initializer timing, snapshots.
//! Scenario IDs refer to `v4/TEST_MATRIX.md`.

use concord::Text;
mod support;

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest;

static ROOT_INIT: AtomicUsize = AtomicUsize::new(0);
static SCOPE_INIT: AtomicUsize = AtomicUsize::new(0);
static ENDPOINT_INIT: AtomicUsize = AtomicUsize::new(0);

fn root_stamp() -> u32 {
    ROOT_INIT.fetch_add(1, Ordering::SeqCst) as u32 + 100
}

fn scope_stamp() -> u32 {
    SCOPE_INIT.fetch_add(1, Ordering::SeqCst) as u32 + 200
}

fn endpoint_stamp() -> u32 {
    ENDPOINT_INIT.fetch_add(1, Ordering::SeqCst) as u32 + 300
}

/// An explicit wire spelling for an optional payload: `None` is written `null`.
#[derive(Clone, Debug)]
pub struct Nullable(pub Option<String>);

impl fmt::Display for Nullable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(s) => f.write_str(s),
            None => f.write_str("null"),
        }
    }
}

/// A validated identifier; only constructible explicitly.
#[derive(Clone, Debug)]
pub struct UserId(pub u64);

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "u{}", self.0)
    }
}

concord::api! {
    /// Value-semantics fixture.
    pub client Vals(
        query tenant: String,
        query("v") version: u32 = 1,
        query locale?: String = "en",
        query trace?: String,
    ) {
        base "http://values.invalid";

        scope region(
            region: String,
            query lang?: String,
        ) {
            path "{region}";

            GET items(
                query cursor?: String,
                query limit: u32 = 20,
                query filter: Nullable,
                query mark?: Nullable,
            ) {
                path "items";
            }
        }

        GET user(user: UserId) {
            path "users/{user}";
        }
    }
}

fn api() -> Vals {
    Vals::new(
        reqwest::Client::new(),
        ValsConfig {
            tenant: "acme".into(),
        },
    )
}

fn query(prepared: &concord::Prepared<()>) -> String {
    prepared.request().url().query().unwrap_or("").to_owned()
}

fn pairs(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|s| !s.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (k.to_owned(), v.to_owned())
        })
        .collect()
}

fn get<'a>(pairs: &'a [(String, String)], key: &str) -> Vec<&'a str> {
    pairs
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .collect()
}

/// V01: a required root value is a positional argument; an empty string is a valid value.
#[test]
fn v01_required_root_value_empty_string_is_valid() {
    let p = Vals::new(
        reqwest::Client::new(),
        ValsConfig {
            tenant: String::new(),
        },
    )
    .region("eu")
    .items(Nullable(None))
    .prepare()
    .unwrap();
    assert_eq!(get(&pairs(&query(&p)), "tenant"), vec![""]);
}

/// V02: an initialized value is sent when not set, and the set value replaces it.
#[test]
fn v02_initialized_value_and_override() {
    let p = api().region("eu").items(Nullable(None)).prepare().unwrap();
    assert_eq!(get(&pairs(&query(&p)), "v"), vec!["1"]);
    let p = api()
        .set_version(2)
        .region("eu")
        .items(Nullable(None))
        .prepare()
        .unwrap();
    assert_eq!(get(&pairs(&query(&p)), "v"), vec!["2"]);
}

/// V03 + V09: initializers run exactly once per owner construction — `new`, each scope entry,
/// each endpoint call — and never on clone, setters, prepare or send. Uses its own client and
/// counters so that concurrently running tests cannot interfere.
#[test]
fn v03_initializer_evaluation_points() {
    let delta = || {
        (
            ROOT_INIT.load(Ordering::SeqCst),
            SCOPE_INIT.load(Ordering::SeqCst),
            ENDPOINT_INIT.load(Ordering::SeqCst),
        )
    };
    assert_eq!(delta(), (0, 0, 0));

    let api = Timing::new(reqwest::Client::new());
    assert_eq!(delta(), (1, 0, 0));
    let api2 = api.clone().set_root(9);
    assert_eq!(delta(), (1, 0, 0), "clone and setters do not re-evaluate");
    let eu = api2.inner();
    let _us = api2.inner();
    assert_eq!(
        delta(),
        (1, 2, 0),
        "each scope entry evaluates its own initializers once"
    );
    let eu2 = eu.clone().set_scoped(1);
    assert_eq!(delta(), (1, 2, 0));
    let req = eu2.leaf();
    let req2 = req.clone();
    assert_eq!(
        delta(),
        (1, 2, 1),
        "each endpoint call evaluates its initializers once"
    );
    let a = req.clone().prepare().unwrap();
    let b = req.prepare().unwrap();
    let c = req2.prepare().unwrap();
    assert_eq!(delta(), (1, 2, 1), "prepare never evaluates initializers");
    let stamp = |p: &concord::Prepared<()>| get(&pairs(&query(p)), "e").concat();
    assert_eq!(stamp(&a), "300");
    assert_eq!(stamp(&a), stamp(&b));
    assert_eq!(stamp(&a), stamp(&c));
}

concord::api! {
    /// Initializer-timing fixture (used only by `v03_initializer_evaluation_points`).
    pub client Timing(query("r") root: u32 = root_stamp()) {
        base "http://timing.invalid";
        scope inner(query("s") scoped: u32 = scope_stamp()) {
            GET leaf(query("e") stamp: u32 = endpoint_stamp());
        }
    }
}

/// V04: an absent optional value emits no occurrence; a present empty string emits `key=`.
#[test]
fn v04_optional_absent_vs_present_empty() {
    let p = api().region("eu").items(Nullable(None)).prepare().unwrap();
    assert!(get(&pairs(&query(&p)), "cursor").is_empty());
    let p = api()
        .region("eu")
        .items(Nullable(None))
        .set_cursor("")
        .prepare()
        .unwrap();
    assert_eq!(get(&pairs(&query(&p)), "cursor"), vec![""]);
}

/// V05 + C11: a required value whose payload is optional is still required; its wire spelling is
/// the type's own `Display` (here `null`), never "omit".
#[test]
fn v05_required_optional_payload_is_not_omission() {
    let p = api().region("eu").items(Nullable(None)).prepare().unwrap();
    assert_eq!(get(&pairs(&query(&p)), "filter"), vec!["null"]);
    let p = api()
        .region("eu")
        .items(Nullable(Some(String::new())))
        .prepare()
        .unwrap();
    assert_eq!(get(&pairs(&query(&p)), "filter"), vec![""]);
}

/// V06: an optional value with an optional payload has three distinguishable states.
#[test]
fn v06_nested_absence() {
    let base = || api().region("eu").items(Nullable(None));
    let absent = base().prepare().unwrap();
    let inner_none = base().set_mark(Nullable(None)).prepare().unwrap();
    let inner_empty = base()
        .set_mark(Nullable(Some(String::new())))
        .prepare()
        .unwrap();
    assert!(get(&pairs(&query(&absent)), "mark").is_empty());
    assert_eq!(get(&pairs(&query(&inner_none)), "mark"), vec!["null"]);
    assert_eq!(get(&pairs(&query(&inner_empty)), "mark"), vec![""]);
}

/// V07: `maybe_x(None)` makes the value absent (it is not "set if some"); `maybe_x(Some(v))`
/// equals `set_x(v)`.
#[test]
fn v07_maybe_is_exact() {
    let p = api()
        .maybe_locale(None)
        .region("eu")
        .items(Nullable(None))
        .prepare()
        .unwrap();
    assert!(get(&pairs(&query(&p)), "locale").is_empty());
    let p = api().region("eu").items(Nullable(None)).prepare().unwrap();
    assert_eq!(get(&pairs(&query(&p)), "locale"), vec!["en"]);
    let p = api()
        .maybe_locale(Some("fr".to_owned()))
        .region("eu")
        .items(Nullable(None))
        .prepare()
        .unwrap();
    assert_eq!(get(&pairs(&query(&p)), "locale"), vec!["fr"]);
}

/// V10: an endpoint captured before its parents change keeps the values it captured.
#[test]
fn v10_snapshots_are_not_changed_by_later_parent_changes() {
    let api = api();
    let eu = api.region("eu").set_lang("fr");
    let req = eu.items(Nullable(None));
    let _api_changed = api.clone().set_version(7);
    let _eu_changed = eu.clone().set_lang("de");
    let q = pairs(&query(&req.prepare().unwrap()));
    assert_eq!(get(&q, "v"), vec!["1"]);
    assert_eq!(get(&q, "lang"), vec!["fr"]);
}

/// C12: clients built from the same reqwest client share no Concord state.
#[test]
fn c12_no_shared_state_between_clients() {
    let http = reqwest::Client::new();
    let a = Vals::new(http.clone(), ValsConfig { tenant: "a".into() });
    let b = Vals::new(http, ValsConfig { tenant: "b".into() }).set_version(5);
    let qa = pairs(&query(
        &a.region("eu").items(Nullable(None)).prepare().unwrap(),
    ));
    let qb = pairs(&query(
        &b.region("eu").items(Nullable(None)).prepare().unwrap(),
    ));
    assert_eq!((get(&qa, "tenant"), get(&qa, "v")), (vec!["a"], vec!["1"]));
    assert_eq!((get(&qb, "tenant"), get(&qb, "v")), (vec!["b"], vec!["5"]));
}

/// V12: a newtype is supplied explicitly and formatted by its own `Display`.
#[test]
fn v12_newtype_value() {
    let p = api().user(UserId(42)).prepare().unwrap();
    assert_eq!(p.request().url().path(), "/users/u42");
}

/// Declaration order is wire order (root → scope → endpoint; direct placements in order).
#[test]
fn query_order_follows_declaration_order() {
    let p = api()
        .set_trace("t")
        .region("eu")
        .set_lang("de")
        .items(Nullable(None))
        .set_cursor("c")
        .prepare()
        .unwrap();
    let keys: Vec<String> = pairs(&query(&p)).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        [
            "tenant", "v", "locale", "trace", "lang", "cursor", "limit", "filter"
        ]
    );
}

static ORDER_LOG: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

fn logged(tag: &'static str, value: u32) -> u32 {
    ORDER_LOG.lock().unwrap().push(tag);
    value
}

concord::api! {
    /// Initializer order and clone retention (own counters, so no other test interferes).
    pub client Ordered(query("a") a: u32 = logged("a", 1), query("b") b: u32 = logged("b", 2)) {
        base "http://ordered.invalid";
        scope inner(query("s") s: u32 = logged("s", 3)) {
            GET leaf(query("e") e: u32 = logged("e", 4));
        }
    }
}

/// V03: initializers run in declaration order, even when immediately overridden, and `send` does
/// not run them. V09: a cloned scope keeps the values set before cloning.
#[tokio::test]
async fn v03_v09_initializer_order_and_clone_retention() {
    let server = support::Server::reply(support::Reply::new(204)).await;
    let api = Ordered::new(reqwest::Client::new())
        .with_base_url(server.url())
        .unwrap()
        .set_a(10);
    assert_eq!(
        *ORDER_LOG.lock().unwrap(),
        ["a", "b"],
        "both run, in order, despite set_a"
    );
    let inner = api.inner().set_s(30);
    let cloned = inner.clone();
    let leaf = cloned.leaf();
    assert_eq!(*ORDER_LOG.lock().unwrap(), ["a", "b", "s", "e"]);
    let p = leaf.clone().prepare().unwrap();
    assert_eq!(query(&p), "a=10&b=2&s=30&e=4");
    // `send` evaluates nothing either, and sends the retained values (local server, no DNS).
    leaf.send().await.unwrap();
    assert_eq!(ORDER_LOG.lock().unwrap().len(), 4);
    assert_eq!(server.single().query(), Some("a=10&b=2&s=30&e=4"));
}

/// V12: `&String` converts to `String`; literals infer their exact types.
#[test]
fn v12_borrowed_string_and_literals() {
    let owned = String::from("from-ref");
    let p = api()
        .region(&owned)
        .items(Nullable(None))
        .set_cursor(&owned)
        .set_limit(5)
        .prepare()
        .unwrap();
    assert_eq!(p.request().url().path(), "/from-ref/items");
    assert_eq!(get(&pairs(&query(&p)), "cursor"), vec!["from-ref"]);
    assert_eq!(get(&pairs(&query(&p)), "limit"), vec!["5"]);
}

/// A move-only value: never `Clone`.
pub struct Nonce(pub String);

impl fmt::Display for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

static CLONE_KEY_INIT: AtomicUsize = AtomicUsize::new(0);

fn clone_key() -> u32 {
    CLONE_KEY_INIT.fetch_add(1, Ordering::SeqCst) as u32 + 500
}

concord::api! {
    /// Request `Clone` follows the data (own counter, so no other test interferes).
    pub client Cloning {
        base "http://cloning.invalid";
        /// Body + Clone values: `Clone`; the clone keeps the evaluated idempotency key.
        POST submit(header("idempotency-key") key: u32 = clone_key(), body text: String) {
            request Text;
            path "submit";
        }
        /// A move-only value: accepted, the request is simply not `Clone`.
        GET once(query nonce: Nonce) { path "once"; }
    }
}

/// V10 / D08: an endpoint with a body is `Clone` when its values and body are; cloning neither
/// re-evaluates initializers nor changes the body; a move-only value is accepted without `Clone`.
#[test]
fn v10_request_clone_follows_the_data() {
    let api = Cloning::new(reqwest::Client::new());
    let request = api.submit("payload");
    let retry = request.clone();
    assert_eq!(CLONE_KEY_INIT.load(Ordering::SeqCst), 1);
    let a = request.prepare().unwrap();
    let b = retry.prepare().unwrap();
    for p in [&a, &b] {
        assert_eq!(p.request().headers()["idempotency-key"], "500");
        let body = p.request().body().and_then(|b| b.as_bytes()).unwrap();
        assert_eq!(body, b"payload");
    }
    let once = api.once(Nonce("n-1".into())).prepare().unwrap();
    assert_eq!(once.request().url().query(), Some("nonce=n-1"));
}

concord::api! {
    /// Key rotation through the setter of a required value.
    pub client Rotating(#[sensitive] header("x-api-key") mut key: String) {
        base "http://rotating.invalid";
        scope tenant(mut tenant: String) {
            path "t/{tenant}";
            GET item(id: u32, body note: String) {
                request Text;
                path "items/{id}";
            }
        }
    }
}

/// D01 + fresh-use friction: every value has `set_x`, including required ones declared `mut`. Replacing the key
/// keeps the runtime base URL and every other choice; handles and requests created before keep
/// the old key (snapshots).
#[tokio::test]
async fn required_values_have_setters() {
    let server = support::Server::reply(support::Reply::new(204)).await;
    let api = Rotating::new(reqwest::Client::new(), RotatingConfig { key: "old".into() })
        .with_base_url(server.url())
        .unwrap();
    let before = api.tenant("acme").item(1, "a".to_owned());

    let api = api.set_key("new");
    api.tenant("acme")
        .set_tenant("beta")
        .item(1, "a".to_owned())
        .set_id(2)
        .set_note("b".to_owned())
        .send()
        .await
        .unwrap();
    before.send().await.unwrap();

    let requests = server.requests();
    assert_eq!(requests[0].target, "/t/beta/items/2");
    assert_eq!(requests[0].header("x-api-key"), Some("new"));
    assert_eq!(requests[0].body, b"b");
    assert_eq!(requests[1].target, "/t/acme/items/1");
    assert_eq!(requests[1].header("x-api-key"), Some("old"));
}
