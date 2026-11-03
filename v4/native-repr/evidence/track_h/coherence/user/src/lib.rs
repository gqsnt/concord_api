//! A downstream crate exercising each design with code shaped like `api!` output: owner state
//! behind an `Arc`, values read by reference (`&__o0.x`, `if let Some(__v) = &__o0.x`), `each`
//! iterating `&collection`, header names from statics. Dependencies are renamed
//! (`concord4` = design A, `hyperium` = `http`, `rq` = `reqwest`) to show that no type is
//! recognized by its spelling.
#![allow(dead_code)]

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use hyperium::HeaderName;

/// A `HeaderValue` reached through three different paths and an alias: one type.
pub type Hv = hyperium::HeaderValue;
pub static ACCEPT_JSON: rq::header::HeaderValue =
    rq::header::HeaderValue::from_static("application/json");
pub const ACCEPT_TEXT: ::hyperium::header::HeaderValue =
    ::hyperium::header::HeaderValue::from_static("text/plain");

static NAME: HeaderName = HeaderName::from_static("x-v");

/// A local domain enum (custom spelling).
#[derive(Clone, Copy, Debug)]
pub enum Sort {
    Newest,
    PriceAsc,
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Newest => "newest",
            Sort::PriceAsc => "price_asc",
        })
    }
}

/// Bytes valid for `HeaderValue` but not UTF-8 / visible ASCII (H5).
pub fn latin1() -> Hv {
    Hv::from_bytes(b"caf\xe9").unwrap()
}

/// A value whose native sensitivity flag is set (H6).
pub fn flagged(text: &'static str) -> Hv {
    let mut v = Hv::from_static(text);
    v.set_sensitive(true);
    v
}

// ============================================================================================
// Design A
// ============================================================================================
pub mod a {
    use super::*;
    use concord4::{DebugHeader, Invalid, Parts, ToHeaderValue};

    impl ToHeaderValue for Sort {
        fn to_header_value(&self) -> Option<concord4::HeaderValue> {
            Some(concord4::HeaderValue::from_static(match self {
                Sort::Newest => "newest",
                Sort::PriceAsc => "price_asc",
            }))
        }
    }

    /// A foreign `Display` type through a local newtype.
    #[derive(Clone)]
    pub struct Ip(pub std::net::IpAddr);
    impl ToHeaderValue for Ip {
        fn to_header_value(&self) -> Option<concord4::HeaderValue> {
            self.0.to_string().to_header_value()
        }
    }

    #[derive(Clone)]
    pub struct ClientState {
        pub owned: hyperium::HeaderValue,
        pub alias: Hv,
        pub qualified: ::rq::header::HeaderValue,
        pub stat: concord4::HeaderValue,
        pub opt: Option<Hv>,
        pub list: Vec<Hv>,
        pub maybe_one: Option<Hv>,
        pub bytes: Hv,
        pub secret: Hv,
        pub n: u32,
        pub small: u8,
        pub flag: bool,
        pub text: String,
        pub lit: &'static str,
        pub boxed: Box<str>,
        pub shared: Arc<str>,
        pub cow: Cow<'static, str>,
        pub sort: Sort,
        pub sorts: Vec<Sort>,
        pub ip: Ip,
        pub nums: Vec<u64>,
        pub strs: Vec<&'static str>,
    }

    pub struct Client {
        pub __s: Arc<ClientState>,
    }

    impl Client {
        pub fn new() -> Self {
            Client {
                __s: Arc::new(ClientState {
                    owned: hyperium::HeaderValue::from_str("owned").unwrap(),
                    alias: ACCEPT_TEXT,
                    qualified: ACCEPT_JSON.clone(),
                    stat: concord4::HeaderValue::from_static("static"),
                    opt: Some(ACCEPT_JSON.clone()),
                    list: vec![Hv::from_static("a"), Hv::from_static("b")],
                    maybe_one: None,
                    bytes: latin1(),
                    secret: flagged("s3cr3t"),
                    n: 7,
                    small: 3,
                    flag: true,
                    text: "text".to_owned(),
                    lit: "lit",
                    boxed: "boxed".into(),
                    shared: "shared".into(),
                    cow: Cow::Borrowed("cow"),
                    sort: Sort::PriceAsc,
                    sorts: vec![Sort::Newest, Sort::PriceAsc],
                    ip: Ip("127.0.0.1".parse().unwrap()),
                    nums: vec![1, 2],
                    strs: vec!["x", "y"],
                }),
            }
        }

        /// Shaped like a generated `prepare`: every read borrows owner state.
        pub fn prepare(&self) -> Result<Parts, Invalid> {
            let __o0 = &*self.__s;
            let mut __p = Parts::default();
            let n = || ::core::clone::Clone::clone(&NAME);
            __p.header(n(), "owned", &__o0.owned, false)?;
            __p.header(n(), "alias", &__o0.alias, false)?;
            __p.header(n(), "qualified", &__o0.qualified, false)?;
            __p.header(n(), "stat", &__o0.stat, false)?;
            __p.header(n(), "static item", &ACCEPT_JSON, false)?;
            if let ::core::option::Option::Some(__v) = &__o0.opt {
                __p.header(n(), "opt", &*__v, false)?;
            }
            __p.header_each(n(), "list", &__o0.list, false)?;
            __p.header_each(n(), "maybe_one", &__o0.maybe_one, false)?;
            __p.header(n(), "bytes", &__o0.bytes, false)?;
            __p.header(n(), "secret", &__o0.secret, false)?;
            __p.header(n(), "n", &__o0.n, false)?;
            __p.header(n(), "small", &__o0.small, false)?;
            __p.header(n(), "flag", &__o0.flag, false)?;
            __p.header(n(), "text", &__o0.text, true)?;
            __p.header(n(), "lit", &__o0.lit, false)?;
            __p.header(n(), "literal", "application/json", false)?;
            __p.header(n(), "boxed", &__o0.boxed, false)?;
            __p.header(n(), "shared", &__o0.shared, false)?;
            __p.header(n(), "cow", &__o0.cow, false)?;
            __p.header(n(), "sort", &__o0.sort, false)?;
            __p.header_each(n(), "sorts", &__o0.sorts, false)?;
            __p.header(n(), "ip", &__o0.ip, false)?;
            __p.header_each(n(), "nums", &__o0.nums, false)?;
            __p.header_each(n(), "strs", &__o0.strs, false)?;
            Ok(__p)
        }
    }

    impl fmt::Debug for Client {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let __s = &*self.__s;
            let mut __d = f.debug_struct("Client");
            __d.field("owned", &DebugHeader(&__s.owned));
            __d.field("opt", &__s.opt.as_ref().map(DebugHeader));
            __d.field("bytes", &DebugHeader(&__s.bytes));
            __d.field("secret", &DebugHeader(&__s.secret));
            __d.field("n", &DebugHeader(&__s.n));
            __d.field("text", &DebugHeader(&__s.text));
            __d.finish()
        }
    }

    /// Integer literals reach the trait only through declared types; a bare literal falls back
    /// to `i32` (probe: does it select the `i32` impl or report ambiguity?).
    pub fn literal_inference() -> Option<concord4::HeaderValue> {
        fn take<T: ToHeaderValue + ?Sized>(v: &T) -> Option<concord4::HeaderValue> {
            v.to_header_value()
        }
        take(&7)
    }

    /// `HeaderValue::from(7)` in an initializer: `http`'s `From<{integer}>` impls + fallback.
    pub fn literal_from() -> Hv {
        Hv::from(7)
    }

    #[cfg(feature = "neg_a_foreign_display")]
    pub fn foreign(p: &mut Parts, ip: &std::net::IpAddr) {
        let _ = p.header(NAME.clone(), "ip", ip, false);
    }

    #[cfg(feature = "neg_a_option")]
    pub fn option(p: &mut Parts, v: &Option<Hv>) {
        let _ = p.header(NAME.clone(), "opt", v, false);
    }

    #[cfg(feature = "neg_a_orphan")]
    impl ToHeaderValue for std::net::IpAddr {
        fn to_header_value(&self) -> Option<concord4::HeaderValue> {
            None
        }
    }

    #[cfg(feature = "neg_a_float")]
    pub fn float(p: &mut Parts, v: &f64) {
        let _ = p.header(NAME.clone(), "ratio", v, false);
    }

    /// H10: a native value placed in a query (`Display` route).
    #[cfg(feature = "neg_a_query")]
    pub fn query(p: &mut Parts, v: &Hv) -> String {
        p.query(v)
    }

    #[cfg(feature = "neg_a_literal_infer")]
    pub fn ambiguous() {
        fn take<T: ToHeaderValue + ?Sized>(_: &T) {}
        // A float literal: no impl at all.
        take(&1.5);
    }

    #[cfg(feature = "neg_marker")]
    pub mod marker {
        use super::*;
        pub struct Region(pub &'static str);
        impl fmt::Display for Region {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.0)
            }
        }
        impl concord4::HeaderText for Region {}
        pub fn region() -> Option<concord4::HeaderValue> {
            Region("eu-west").to_header_value()
        }
    }
}

// ============================================================================================
// Design B
// ============================================================================================
pub mod b {
    use super::*;
    use design_b::{Invalid, Parts};

    impl From<&Sort> for hyperium::HeaderValue {
        fn from(s: &Sort) -> Self {
            hyperium::HeaderValue::from_static(match s {
                Sort::Newest => "newest",
                Sort::PriceAsc => "price_asc",
            })
        }
    }

    pub struct State {
        pub native: Hv,
        pub opt: Option<Hv>,
        pub list: Vec<Hv>,
        pub text: String,
        pub lit: &'static str,
        pub n: u64,
        pub sort: Sort,
        pub bytes: Hv,
        pub secret: Hv,
        pub nonascii: String,
    }

    pub fn state() -> State {
        State {
            native: ACCEPT_JSON.clone(),
            opt: Some(ACCEPT_TEXT),
            list: vec![Hv::from_static("a"), Hv::from_static("b")],
            text: "text".into(),
            lit: "lit",
            n: 7,
            sort: Sort::Newest,
            bytes: latin1(),
            secret: flagged("s3cr3t"),
            nonascii: "café".into(),
        }
    }

    pub fn prepare(__o0: &State) -> Result<Parts, Invalid> {
        let mut __p = Parts::default();
        let n = || NAME.clone();
        __p.header_ref(n(), "native", &__o0.native, false)?;
        if let Some(__v) = &__o0.opt {
            __p.header_ref(n(), "opt", &*__v, false)?;
        }
        __p.header_each_ref(n(), "list", &__o0.list, false)?;
        __p.header_ref(n(), "text", &__o0.text, false)?;
        __p.header_ref(n(), "lit", __o0.lit, false)?;
        __p.header_owned(n(), "n", &__o0.n, false)?;
        __p.header_ref(n(), "sort", &__o0.sort, false)?;
        __p.header_ref(n(), "bytes", &__o0.bytes, false)?;
        __p.header_ref(n(), "secret", &__o0.secret, false)?;
        // http's text conversions accept obs-text: "café" is sent as UTF-8 bytes.
        __p.header_ref(n(), "nonascii", &__o0.nonascii, false)?;
        Ok(__p)
    }

    /// `each` over integers needs owned items; the by-reference bound does not hold for `&u64`.
    #[cfg(feature = "neg_b_int_ref")]
    pub fn int_ref(p: &mut Parts, nums: &Vec<u64>) {
        let _ = p.header_each_ref(NAME.clone(), "nums", nums, false);
    }

    /// A local `Display` type without a `From` impl.
    #[cfg(feature = "neg_b_local_display")]
    pub fn local_display(p: &mut Parts, v: &Region) {
        let _ = p.header_ref(NAME.clone(), "region", v, false);
    }
    #[cfg(feature = "neg_b_local_display")]
    pub struct Region;
    #[cfg(feature = "neg_b_local_display")]
    impl fmt::Display for Region {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("eu")
        }
    }

    #[cfg(feature = "neg_b_orphan")]
    impl From<&std::net::IpAddr> for hyperium::HeaderValue {
        fn from(_: &std::net::IpAddr) -> Self {
            hyperium::HeaderValue::from_static("x")
        }
    }
}

// ============================================================================================
// Design C
// ============================================================================================
pub mod c {
    use super::*;
    use design_c::{DisplayRoute as _, Invalid, NativeRoute as _, Parts, Probe};

    pub struct State {
        pub native: Hv,
        pub alias: hyperium::HeaderValue,
        pub list: Vec<Hv>,
        pub text: String,
        pub n: u32,
        pub sort: Sort,
        pub bytes: Hv,
        pub secret: Hv,
    }

    pub fn state() -> State {
        State {
            native: ACCEPT_JSON.clone(),
            alias: ACCEPT_TEXT,
            list: vec![Hv::from_static("a"), Hv::from_static("b")],
            text: "text".into(),
            n: 7,
            sort: Sort::PriceAsc,
            bytes: latin1(),
            secret: flagged("s3cr3t"),
        }
    }

    pub fn prepare(__o0: &State) -> Result<Parts, Invalid> {
        let mut __p = Parts::default();
        let n = || NAME.clone();
        __p.header(n(), "native", (&Probe(&__o0.native)).__header(), false)?;
        __p.header(n(), "alias", (&Probe(&__o0.alias)).__header(), false)?;
        for __i in &__o0.list {
            __p.header(n(), "list", (&Probe(__i)).__header(), false)?;
        }
        __p.header(n(), "text", (&Probe(&__o0.text)).__header(), false)?;
        __p.header(n(), "n", (&Probe(&__o0.n)).__header(), false)?;
        __p.header(n(), "sort", (&Probe(&__o0.sort)).__header(), false)?;
        __p.header(n(), "bytes", (&Probe(&__o0.bytes)).__header(), false)?;
        __p.header(n(), "secret", (&Probe(&__o0.secret)).__header(), false)?;
        // An integer literal: `{integer}` is not a concrete type during method lookup.
        __p.header(n(), "lit", (&Probe(&7)).__header(), false)?;
        Ok(__p)
    }

    /// Failure mode: inside generic code the native impl is invisible — the same call takes the
    /// `Display` route (and here fails to compile for `HeaderValue`).
    #[cfg(feature = "neg_c_generic")]
    pub fn generic<T>(v: &T) -> Option<hyperium::HeaderValue> {
        (&Probe(v)).__header()
    }

    /// A type with neither route.
    #[cfg(feature = "neg_c_neither")]
    pub fn neither(v: &Option<Hv>) -> Option<hyperium::HeaderValue> {
        (&Probe(v)).__header()
    }
}

// ============================================================================================
// Design D
// ============================================================================================
pub mod d {
    use super::*;
    use design_d::{Invalid, Parts};

    pub struct State {
        pub native: Hv,
        pub boxed: Box<Hv>,
        pub shared: Arc<rq::header::HeaderValue>,
        pub list: Vec<Hv>,
        pub text: String,
        pub n: u32,
        pub sort: Sort,
        pub bytes: Hv,
        pub secret: Hv,
    }

    pub fn state() -> State {
        State {
            native: ACCEPT_JSON.clone(),
            boxed: Box::new(ACCEPT_TEXT),
            shared: Arc::new(ACCEPT_JSON.clone()),
            list: vec![Hv::from_static("a"), Hv::from_static("b")],
            text: "text".into(),
            n: 7,
            sort: Sort::PriceAsc,
            bytes: latin1(),
            secret: flagged("s3cr3t"),
        }
    }

    pub fn prepare(__o0: &State) -> Result<Parts, Invalid> {
        let mut __p = Parts::default();
        let n = || NAME.clone();
        // `native` in the declaration:
        __p.header_native(n(), "native", &__o0.native, false)?;
        __p.header_native(n(), "boxed", &*__o0.boxed, false)?;
        __p.header_native(n(), "shared", &*__o0.shared, false)?;
        for __i in &__o0.list {
            __p.header_native(n(), "list", __i, false)?;
        }
        __p.header_native(n(), "bytes", &__o0.bytes, false)?;
        __p.header_native(n(), "secret", &__o0.secret, false)?;
        // no keyword (today's `Display` route):
        __p.header_text(n(), "text", &__o0.text, false)?;
        __p.header_text(n(), "n", &__o0.n, false)?;
        __p.header_text(n(), "sort", &__o0.sort, false)?;
        Ok(__p)
    }

    /// The author forgot `native`: does the Concord trait's hint appear?
    #[cfg(feature = "neg_d_missing_keyword")]
    pub fn missing(p: &mut Parts, v: &Hv) {
        let _ = p.header_text(NAME.clone(), "accept", v, false);
    }

    /// Same, with today's plain `Display` bound.
    #[cfg(feature = "neg_d_missing_keyword_plain")]
    pub fn missing_plain(p: &mut Parts, v: &Hv) {
        let _ = p.header_plain(NAME.clone(), "accept", v, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(map: &hyperium::HeaderMap) -> Vec<(&[u8], bool)> {
        map.get_all("x-v").iter().map(|v| (v.as_bytes(), v.is_sensitive())).collect()
    }

    #[test]
    fn design_a() {
        let client = a::Client::new();
        let p = client.prepare().unwrap();
        let got = values(&p.headers);
        let expected: Vec<(&[u8], bool)> = vec![
            (b"owned", false),
            (b"text/plain", false),
            (b"application/json", false),
            (b"static", false),
            (b"application/json", false),
            (b"application/json", false),
            (b"a", false),
            (b"b", false),
            (b"caf\xe9", false),
            (b"s3cr3t", true), // native flag kept without a declaration marker
            (b"7", false),
            (b"3", false),
            (b"true", false),
            (b"text", true), // declaration classification added
            (b"lit", false),
            (b"application/json", false),
            (b"boxed", false),
            (b"shared", false),
            (b"cow", false),
            (b"price_asc", false),
            (b"newest", false),
            (b"price_asc", false),
            (b"127.0.0.1", false),
            (b"1", false),
            (b"2", false),
            (b"x", false),
            (b"y", false),
        ];
        assert_eq!(got, expected);
        // The caller's value is not mutated by adding the declaration's flag.
        assert!(!client.__s.text.is_empty());
        // Debug without `Display`, native flag respected, non-UTF-8 escaped.
        assert_eq!(
            format!("{client:?}"),
            r#"Client { owned: "owned", opt: Some("application/json"), bytes: "caf\xe9", secret: <redacted>, n: "7", text: "text" }"#
        );
        assert_eq!(a::literal_inference().unwrap(), "7");
        assert_eq!(a::literal_from(), "7");
    }

    #[test]
    fn design_a_text_policy() {
        use concord4::ToHeaderValue;
        assert!("café".to_header_value().is_none(), "text stays visible ASCII");
        assert!("a\tb".to_header_value().is_some());
        assert!(latin1().to_header_value().is_some(), "native bytes pass unchanged");
    }

    #[cfg(feature = "neg_marker")]
    #[test]
    fn design_a_marker() {
        assert_eq!(a::marker::region().unwrap(), "eu-west");
    }

    #[test]
    fn design_b() {
        let p = b::prepare(&b::state()).unwrap();
        let got = values(&p.headers);
        let expected: Vec<(&[u8], bool)> = vec![
            (b"application/json", false),
            (b"text/plain", false),
            (b"a", false),
            (b"b", false),
            (b"text", false),
            (b"lit", false),
            (b"7", false),
            (b"newest", false),
            (b"caf\xe9", false),
            (b"s3cr3t", true),
            ("café".as_bytes(), false), // obs-text accepted: the text policy is gone
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn design_c() {
        let p = c::prepare(&c::state()).unwrap();
        let got = values(&p.headers);
        let expected: Vec<(&[u8], bool)> = vec![
            (b"application/json", false),
            (b"text/plain", false),
            (b"a", false),
            (b"b", false),
            (b"text", false),
            (b"7", false),
            (b"price_asc", false),
            (b"caf\xe9", false),
            (b"s3cr3t", true),
            (b"7", false),
        ];
        assert_eq!(got, expected);
    }

    #[test]
    fn design_d() {
        let p = d::prepare(&d::state()).unwrap();
        let got = values(&p.headers);
        let expected: Vec<(&[u8], bool)> = vec![
            (b"application/json", false),
            (b"text/plain", false),
            (b"application/json", false),
            (b"a", false),
            (b"b", false),
            (b"caf\xe9", false),
            (b"s3cr3t", true),
            (b"text", false),
            (b"7", false),
            (b"price_asc", false),
        ];
        assert_eq!(got, expected);
    }
}
