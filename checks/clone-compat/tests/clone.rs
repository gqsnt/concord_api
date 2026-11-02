//! Positive runtime checks for the conditional `Clone` of endpoint requests (D08).

use std::cell::Cell;

use concord::Prepared;
use concord_clone_compat::{Cloneable as C, MoveOnly as M, Raw};

thread_local! {
    /// Initializer runs on this test thread (each test has its own thread).
    static INIT: Cell<usize> = const { Cell::new(0) };
}

fn fresh_key() -> C {
    let n = INIT.get();
    INIT.set(n + 1);
    C::new(&format!("k{n}"))
}

concord::api! {
    /// Every kind of endpoint data, all `Clone`.
    pub client Shared(header("x-tenant") tenant: C) {
        base "http://shared.invalid/v1";

        /// Required path value, optional query value, initialized header value, `each`
        /// collection and a body.
        POST put(
            id: C,
            query note?: C,
            header("x-key") key: C = fresh_key(),
            query("tag") each tags: Vec<C>,
            body payload: C via Raw,
        ) {
            path "items/{id}";
        }

        /// No values at all: always `Clone`.
        GET ping() { path "ping"; }
    }
}

concord::api! {
    /// The same shape with move-only data: the declaration must keep compiling and preparing.
    pub client Owned(header("x-tenant") tenant: C) {
        base "http://owned.invalid/v1";

        POST put(
            id: M,
            query note?: M,
            header("x-key") key: M = M::new("k"),
            query("tag") each tags: Vec<M>,
            body payload: M via Raw,
        ) {
            path "items/{id}";
        }
    }
}

/// Everything observable about a prepared request.
fn wire<T>(prepared: &Prepared<T>) -> (String, String, Vec<(String, String)>, Vec<u8>) {
    let request = prepared.request();
    let headers = request
        .headers()
        .iter()
        .map(|(n, v)| (n.to_string(), v.to_str().unwrap().to_owned()))
        .collect();
    let body = request.body().and_then(|b| b.as_bytes()).unwrap().to_vec();
    (
        request.method().to_string(),
        request.url().to_string(),
        headers,
        body,
    )
}

fn shared() -> Shared {
    Shared::new(
        concord::reqwest::Client::new(),
        SharedConfig {
            tenant: C::new("acme"),
        },
    )
}

#[test]
fn clone_prepares_identically_without_reevaluating_initializers() {
    let request = shared()
        .put(
            C::new("7"),
            vec![C::new("a"), C::new("b")],
            C::new("payload"),
        )
        .set_note(C::new("n"));
    assert_eq!(INIT.get(), 1, "the endpoint call ran the initializer once");
    let copy = request.clone();
    assert_eq!(INIT.get(), 1, "clone runs no initializer");

    let (a, b) = (request.prepare().unwrap(), copy.prepare().unwrap());
    assert_eq!(wire(&a), wire(&b));
    let (method, url, headers, body) = wire(&a);
    assert_eq!(method, "POST");
    assert_eq!(url, "http://shared.invalid/v1/items/7?note=n&tag=a&tag=b");
    assert!(headers.contains(&("x-tenant".into(), "acme".into())));
    assert!(headers.contains(&("x-key".into(), "k0".into())));
    assert_eq!(body, b"payload");
    assert_eq!(INIT.get(), 1, "prepare runs no initializer");
}

#[test]
fn clones_are_independent() {
    let original = shared().put(C::new("1"), vec![C::new("a")], C::new("one"));
    let changed = original
        .clone()
        .set_id(C::new("2"))
        .maybe_note(Some(C::new("x")))
        .set_key(C::new("other"))
        .set_tags(vec![])
        .set_payload(C::new("two"));
    let (o, c) = (original.prepare().unwrap(), changed.prepare().unwrap());
    let (_, o_url, o_headers, o_body) = wire(&o);
    let (_, c_url, c_headers, c_body) = wire(&c);
    assert_eq!(o_url, "http://shared.invalid/v1/items/1?tag=a");
    assert_eq!(c_url, "http://shared.invalid/v1/items/2?note=x");
    assert_eq!(
        (o_body.as_slice(), c_body.as_slice()),
        (&b"one"[..], &b"two"[..])
    );
    assert!(c_headers.contains(&("x-key".into(), "other".into())));
    assert!(!o_headers.contains(&("x-key".into(), "other".into())));
}

#[test]
fn bodyless_request_is_clone() {
    let request = shared().ping();
    let copy = request.clone();
    let (a, b) = (request.prepare().unwrap(), copy.prepare().unwrap());
    assert_eq!(a.request().url(), b.request().url());
    assert_eq!(a.request().headers(), b.request().headers());
}

/// Canary: if rustc started checking the binder-wrapped bounds at the impl, this declaration
/// (not only `.clone()`) would stop compiling.
#[test]
fn move_only_data_still_declares_and_prepares() {
    let api = Owned::new(
        concord::reqwest::Client::new(),
        OwnedConfig {
            tenant: C::new("acme"),
        },
    );
    let prepared = api
        .put(M::new("9"), vec![M::new("t")], M::new("body"))
        .set_note(M::new("n"))
        .prepare()
        .unwrap();
    let (_, url, headers, body) = wire(&prepared);
    assert_eq!(url, "http://owned.invalid/v1/items/9?note=n&tag=t");
    assert!(headers.contains(&("x-key".into(), "k".into())));
    assert_eq!(body, b"body");
}
