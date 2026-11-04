//! Review of F03 (4.2): a type that contains a fragment forwarded by `macro_rules!` means the same
//! type everywhere the endpoint uses it. `pick!($e * 2)` with `$e = 1 + 2` is `Payload<6>` as a
//! query value, a body and an output; 4.1 named the body's and the output's format statics with a
//! regrouped copy (`Payload<5>`), a type mismatch at the declaration.
#![allow(missing_docs)]

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

macro_rules! pick {
    ($e:expr) => {
        Payload<{ $e }>
    };
}
macro_rules! declare {
    ($e:expr) => {
        concord::api! {
            pub client Forwarded {
                base "http://x.test";
                format Fmt;
                GET query(query p: pick!($e * 2));
                POST body(body p: pick!($e * 2));
                POST optional(body p?: pick!($e * 2));
                GET output() -> pick!($e * 2);
            }
        }
    };
}
declare!(1 + 2);

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
}
