//! F01: every declaration compiles and builds its requests, whichever same-printed payload is
//! `Clone`; requests whose values are all `Clone` are `Clone` and clone to the same request. A
//! `.clone()` of a request holding a non-`Clone` value is rejected (`tests/ui`).

use concord::{Prepared, reqwest};
use concord_clone_hygiene_a as a;
use concord_clone_hygiene_b as b;
use concord_clone_hygiene_c as c;
use concord_clone_hygiene_check::{both_clone, both_plain, direct, forward, reverse, same_type};
use concord_clone_hygiene_d as d;

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn url<T>(p: &Prepared<T>) -> &str {
    p.request().url().as_str()
}

fn body<T>(p: &Prepared<T>) -> Option<&[u8]> {
    p.request().body().map(|b| b.as_bytes().expect("buffered"))
}

const BASE: &str = "http://clone-hygiene.invalid";

#[test]
fn mixed_declarations_compile_and_build_in_both_orders() {
    let api = forward::Probe::new(client());
    let p = api.pair(a::Payload(17), b::Payload(29)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/pair?first=17&second=29"));
    assert_eq!(body(&p), None);
    let p = api.optional().set_second(b::Payload(5)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/optional?second=5"));
    let p = api.body(a::Payload(1), b::Payload(2)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/body?first=1"));
    assert_eq!(body(&p), Some(b"2".as_slice()));

    let api = reverse::Probe::new(client());
    let p = api.pair(b::Payload(29), a::Payload(17)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/pair?first=29&second=17"));
    let p = api.body(b::Payload(3), a::Payload(4)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/body?first=3"));
    assert_eq!(body(&p), Some(b"4".as_slice()));

    let api = direct::Probe::new(client());
    let p = api.pair(a::Payload(1), b::Payload(2)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/pair?first=1&second=2"));
}

#[test]
fn non_clone_controls_build() {
    let api = both_plain::Probe::new(client());
    let p = api.pair(b::Payload(8), d::Payload(9)).prepare().unwrap();
    assert_eq!(url(&p), format!("{BASE}/pair?first=8&second=9"));
    let p = api.body(b::Payload(6), d::Payload(7)).prepare().unwrap();
    assert_eq!(body(&p), Some(b"7".as_slice()));
}

#[test]
fn clone_requests_clone_to_the_same_request() {
    let api = both_clone::Probe::new(client());
    let request = api.pair(a::Payload(1), c::Payload(2));
    let copy = request.clone();
    assert_eq!(
        url(&request.prepare().unwrap()),
        url(&copy.prepare().unwrap())
    );
    let request = api
        .optional()
        .set_first(a::Payload(3))
        .set_second(c::Payload(4));
    let copy = request.clone();
    let (p, q) = (request.prepare().unwrap(), copy.prepare().unwrap());
    assert_eq!(url(&p), format!("{BASE}/optional?first=3&second=4"));
    assert_eq!(url(&p), url(&q));
    let request = api.body(a::Payload(5), c::Payload(6));
    let copy = request.clone();
    let (p, q) = (request.prepare().unwrap(), copy.prepare().unwrap());
    assert_eq!((url(&p), body(&p)), (url(&q), body(&q)));
    assert_eq!(body(&p), Some(b"6".as_slice()));

    let api = same_type::Probe::new(client());
    let request = api.pair(a::Payload(10), a::Payload(11));
    let copy = request.clone();
    assert_eq!(
        url(&request.prepare().unwrap()),
        url(&copy.prepare().unwrap())
    );
}

/// The intended negative: cloning a request that holds a non-`Clone` value does not compile,
/// in each order and form. The `.stderr` files name the non-`Clone` type.
#[test]
fn clone_of_non_clone_request_is_rejected() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

/// Review finding SOL-4 (4.4 review): a payload type may itself bind the lifetime `'__concord`
/// (`for<'__concord> fn(&'__concord str)` is valid Rust and `Clone`). The generated conditional
/// `Clone` bound adds its own unused higher-ranked binder; it must not shadow the payload's
/// lifetime (that was E0496), so the request stays cloneable.
mod lifetime_named_like_the_binder {
    struct Wire;

    impl<T> concord::Encode<T> for Wire {
        const CONTENT_TYPE: Option<&'static str> = None;
        fn encode(_: T) -> Result<concord::reqwest::Body, concord::CodecError> {
            Ok("callback".into())
        }
    }

    concord::api! {
        pub client CallbackApi {
            base "http://127.0.0.1";
            POST send_callback(body callback: for<'__concord> fn(&'__concord str)) {
                path "callback";
                request Wire;
            }
            POST send_both(
                body callback: for<'__concord, '__concord1> fn(&'__concord str, &'__concord1 str),
            ) {
                path "both";
                request Wire;
            }
        }
    }

    #[test]
    fn a_payload_lifetime_named_like_the_binder_keeps_clone() {
        let callback: for<'x> fn(&'x str) = |_| {};
        let request = CallbackApi::new(concord::reqwest::Client::new()).send_callback(callback);
        let _ = request.clone().prepare().unwrap();
        let both: for<'x, 'y> fn(&'x str, &'y str) = |_, _| {};
        let request = CallbackApi::new(concord::reqwest::Client::new()).send_both(both);
        let _ = request.clone().prepare().unwrap();
    }
}
