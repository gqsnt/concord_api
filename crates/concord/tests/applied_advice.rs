//! F03/F04 (4.2): the migration advice in `tests/ui/migration_{literal,raw}_advice.stderr`,
//! applied as code, compiles and keeps the declaration's meaning: the const expression still counts
//! six bytes in `"a :: b"` (4.1 suggested `"a::b"`, four), and the raw body name stays `r#type`
//! (4.1 suggested `body type`, which does not parse). Each applied text is checked against the
//! advice snapshot, so the two cannot drift apart.
#![allow(missing_docs)]

use concord::{CodecError, Encode, reqwest};

pub const fn count(s: &str) -> usize {
    s.len()
}
pub struct Payload<const N: usize>;
pub struct Fmt;
impl<const N: usize> Encode<Payload<N>> for Fmt {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: Payload<N>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(N.to_string()))
    }
}
macro_rules! choose {
    (. .) => {
        Payload<1>
    };
    (..) => {
        Payload<2>
    };
    (a < b < c > > d) => {
        Payload<3>
    };
    (a < b < c >> d) => {
        Payload<4>
    };
    (1. . 2) => {
        Payload<5>
    };
    (1 .. 2) => {
        Payload<7>
    };
}
pub struct Utf8;
impl Encode<String> for Utf8 {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value))
    }
}

concord::api! {
    pub client Applied {
        base "http://x.test";
        POST via_literal(body p: Payload<{ count("a :: b") }>) { request Fmt; }
        POST trailing_literal(body p: Payload<{ count(r#"x , y"#) }>) { request Fmt; }
        POST raw(body r#type?: String) { request Utf8; }
        POST raw_via(body r#match: String) { request Utf8; }
        POST separate(body p: choose!(. .)) { request Fmt; }
        POST angles(body p: choose!(a < b < c > > d)) { request Fmt; }
        POST float(body p: choose!(1. . 2)) { request Fmt; }
    }
}

const LITERAL_ADVICE: &str = include_str!("ui/migration_literal_advice.stderr");
const RAW_ADVICE: &str = include_str!("ui/migration_raw_advice.stderr");
const TOKEN_ADVICE: &str = include_str!("ui/migration_token_advice.stderr");

#[test]
fn applied_literal_advice_keeps_the_const_value() {
    for applied in [
        "write `body p: Payload<{ count(\"a :: b\") }>` and `request Fmt;`",
        "write `body p: Payload<{ count(r#\"x , y\"#) }>` and `request Fmt;`",
    ] {
        assert!(
            LITERAL_ADVICE.contains(applied),
            "{applied}\n{LITERAL_ADVICE}"
        );
    }
    let api = Applied::new(reqwest::Client::new());
    // `Payload::<6>` is the only argument type that compiles: the applied type is `Payload<6>`.
    let p = api.via_literal(Payload::<6>).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"6".as_slice())
    );
    let p = api.trailing_literal(Payload::<5>).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"5".as_slice())
    );
}

#[test]
fn applied_raw_advice_keeps_the_raw_name() {
    for applied in [
        "write `body r#type?: String` and `request Utf8;`",
        "write `body r#match: String` and `request Utf8;`",
    ] {
        assert!(RAW_ADVICE.contains(applied), "{applied}\n{RAW_ADVICE}");
    }
    let api = Applied::new(reqwest::Client::new());
    let p = api.raw().set_type("dummy").prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"dummy".as_slice())
    );
    let p = api.raw().prepare().unwrap();
    assert!(p.request().body().is_none());
    let p = api.raw_via("m".to_owned()).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"m".as_slice())
    );
}

/// Review of F03: applied, the advice selects the same macro arm (`choose!(. .)` is `Payload<1>`,
/// `choose!(..)` would be `<2>`; `a < b < c > > d` is `<3>`, not `<4>`; `1. . 2` is `<5>`, not
/// `<7>`). A forwarded fragment is not spelled: the advice names it `T` and says why.
#[test]
fn applied_token_advice_keeps_the_payload_type() {
    for applied in [
        "write `body p: choose!(. .)` and `request Fmt;`",
        "write `body p: choose!(a < b < c > > d)` and `request Fmt;`",
        "write `body p: choose!(1. . 2)` and `request Fmt;`",
        "write `body p: T` and `request Fmt;` on the endpoint (`T` is the payload type as declared",
    ] {
        assert!(TOKEN_ADVICE.contains(applied), "{applied}\n{TOKEN_ADVICE}");
    }
    // The messages (not the quoted source lines) never spell a forwarded fragment.
    for message in TOKEN_ADVICE.lines().filter(|l| l.starts_with("error:")) {
        assert!(
            !message.contains("pick!(") && !message.contains("Vec<"),
            "{message}"
        );
    }
    let api = Applied::new(reqwest::Client::new());
    for (p, bytes) in [
        (api.separate(Payload::<1>).prepare().unwrap(), b"1"),
        (api.angles(Payload::<3>).prepare().unwrap(), b"3"),
        (api.float(Payload::<5>).prepare().unwrap(), b"5"),
    ] {
        assert_eq!(
            p.request().body().unwrap().as_bytes(),
            Some(bytes.as_slice())
        );
    }
}
