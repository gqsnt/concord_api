//! Review of F03 (4.2): a type or initializer that contains a fragment forwarded by `macro_rules!`
//! means what Rust says it means, everywhere the endpoint uses it. `pick!($e * 2)` with
//! `$e = 1 + 2` is `Payload<6>` as a query value, a body and an output, and `$e * 2` initializes 6;
//! 4.1 named the body's and the output's format statics with a regrouped copy (`Payload<5>`, a type
//! mismatch at the declaration), named `Payload<{ $e * 2 }>` written directly `Payload<5>`,
//! initialized 5, and rejected a forwarded `dyn A + B` behind `&` as an ambiguous `+`.
#![allow(missing_docs)]
// The parentheses Concord adds around a forwarded fragment are never reported as unneeded.
#![deny(unused_parens)]

use concord::{CodecError, Decode, Encode, Prepared, ResponseHead, bytes::Bytes, reqwest};

pub struct Payload<const N: usize>;
impl<const N: usize> core::fmt::Display for Payload<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{N}")
    }
}

pub struct Fmt;
impl<const N: usize> Encode<Payload<N>> for Fmt {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: Payload<N>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(N.to_string()))
    }
}
impl<const N: usize> Decode<Payload<N>> for Fmt {
    type Context = ();
    fn check_content_type(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body(_: (), _: Bytes) -> Result<Payload<N>, CodecError> {
        Ok(Payload)
    }
}

/// Encodes a byte array as its bytes.
pub struct Raw;
impl<const N: usize> Encode<[u8; N]> for Raw {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: [u8; N]) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.to_vec()))
    }
}

/// `owned` for a value, `borrowed` for a reference, taken by value: `$e.meaning()` with
/// `$e = &VALUE` is `(&VALUE).meaning()` in Rust (borrowed), while `&VALUE.meaning()` would borrow
/// the owned answer (and `IntoValue<String>` accepts `&String`, so it would still compile).
pub trait Meaning {
    fn meaning(self) -> String;
}
#[derive(Clone, Copy)]
pub struct Foo;
impl Meaning for Foo {
    fn meaning(self) -> String {
        "owned".into()
    }
}
impl Meaning for &Foo {
    fn meaning(self) -> String {
        "borrowed".into()
    }
}
/// `1` for a value, `2` for a reference: `1usize << $e.weight()` with `$e = &VALUE` is 4 in Rust.
pub trait Weight {
    fn weight(self) -> usize;
}
impl Weight for Foo {
    fn weight(self) -> usize {
        1
    }
}
impl Weight for &Foo {
    fn weight(self) -> usize {
        2
    }
}
pub static VALUE: Foo = Foo;
pub const DATA: [u8; 3] = [7, 8, 9];

/// Encodes a borrowed displayable trait object as its text.
pub struct Shown;
impl Encode<&'static (dyn core::fmt::Display + Sync)> for Shown {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(
        value: &'static (dyn core::fmt::Display + Sync),
    ) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.to_string()))
    }
}

macro_rules! pick {
    ($e:expr) => {
        Payload<{ $e }>
    };
}
macro_rules! declare {
    ($e:expr, $t:ty, $r:expr, $d:expr, $s:ty, $v:ty, $w:ty) => {
        concord::api! {
            pub client Forwarded {
                base "http://x.test";
                format Fmt;
                GET query(query p: pick!($e * 2));
                POST body(body p: pick!($e * 2));
                POST optional(body p?: pick!($e * 2));
                GET output() -> pick!($e * 2);
                GET initialized(query n: usize = $e * 2);
                GET direct(query p: Payload<{ $e * 2 }>);
                POST object(body p: &'static $t) { request Shown; }
                GET method(query n: String = $r.meaning());
                POST length(body p: [u8; $d.len()]) { request Raw; }
                GET alone(query n: usize = $e, query p?: Payload<{ $e }>);
                GET sized(query n: usize = core::mem::size_of::<$s>());
                GET qualified(query n: usize = <$v>::new().len());
                GET cast(query n: u64 = $e as $w);
                GET shift(query n: usize = 1usize << $r.weight());
                GET pointer(query n: usize = (core::ptr::null::<u8>() as *const $w).is_null() as usize);
            }
        }
    };
}
declare!(
    1 + 2,
    dyn core::fmt::Display + Sync,
    &VALUE,
    &DATA,
    &'static str,
    Vec<u8>,
    core::primitive::u64
);

#[test]
fn forwarded_fragments_keep_their_grouping() {
    let api = Forwarded::new(reqwest::Client::new());
    let p = api.query(Payload::<6>).prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("p=6"));
    let p = api.body(Payload::<6>).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"6".as_slice())
    );
    let p = api.optional().set_p(Payload::<6>).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"6".as_slice())
    );
    let _: Prepared<Payload<6>> = api.output().prepare().unwrap();
    // The initializer is the written expression too (4.1 sent 5); its rustdoc says
    // `(1 + 2) * 2` (`concord-macros` unit test `documented_initializers_keep_forwarded_grouping`).
    let p = api.initialized().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=6"));
    // Written directly in a const argument (no inner macro): also `Payload<6>`.
    let p = api.direct(Payload::<6>).prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("p=6"));
    // A forwarded `dyn A + B` behind `&'static` is `&'static (dyn A + B)`, as in Rust (4.1:
    // "ambiguous `+` in a type").
    let value: &'static (dyn core::fmt::Display + Sync) = &7u8;
    let p = api.object(value).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"7".as_slice())
    );
    // A fragment that also reads as a type (`&VALUE`) is still an expression where one is
    // expected: `(&VALUE).meaning()` and `[u8; (&DATA).len()]`, as in Rust.
    let p = api.method().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=borrowed"));
    let p = api.length(DATA).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some([7, 8, 9].as_slice())
    );
    // Used alone, the fragment's value is unchanged (and its parentheses raise no lint).
    let p = api.alone().set_p(Payload::<3>).prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=3&p=3"));
    // A type fragment in a type position within an expression stays a type (no parentheses, so
    // no `unused_parens`): a turbofish, a qualified path's start and the type after `as`.
    let p = api.sized().prepare().unwrap();
    let size = core::mem::size_of::<&'static str>().to_string();
    assert_eq!(
        p.request().url().query(),
        Some(format!("n={size}").as_str())
    );
    let p = api.qualified().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=0"));
    let p = api.cast().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=3"));
    // No guess about which tokens of an expression are types: `<<` is not an opening bracket
    // (`1 << (&VALUE).weight()` is 4), and a type behind `as *const` raises no lint either.
    let p = api.shift().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=4"));
    let p = api.pointer().prepare().unwrap();
    assert_eq!(p.request().url().query(), Some("n=1"));
}
