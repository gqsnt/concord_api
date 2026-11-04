// Review of F03 (4.2): advice shows a macro's input token for token, so it selects the same arm
// (`choose!(. .)` is not `choose!(..)`, `a < b < c > > d` is not `... >> d`, `1. . 2` is not
// `1..2`); a payload holding a fragment forwarded by `macro_rules!` is named `T` instead of spelled
// (no written text is that fragment). tests/applied_advice.rs compiles the applied text.
use concord::{CodecError, Encode, reqwest};
pub struct Payload<const N: usize>;
pub struct Fmt;
impl<const N: usize> Encode<Payload<N>> for Fmt {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: Payload<N>) -> Result<reqwest::Body, CodecError> { Ok(reqwest::Body::from(N.to_string())) }
}
macro_rules! choose {
    (. .) => { Payload<1> };
    (..) => { Payload<2> };
    (a < b < c > > d) => { Payload<3> };
    (a < b < c >> d) => { Payload<4> };
    (1. . 2) => { Payload<5> };
    (1 .. 2) => { Payload<7> };
}
macro_rules! pick { ($e:expr) => { Payload<{ $e }> }; }
concord::api! {
    pub client Separate {
        base "http://x.test";
        POST broken(body p: choose!(. .) via Fmt);
    }
}
concord::api! {
    pub client Angles {
        base "http://x.test";
        POST broken(body p: choose!(a < b < c > > d) via Fmt);
    }
}
concord::api! {
    pub client Float {
        base "http://x.test";
        POST broken(p: choose!(1. . 2) Fmt);
    }
}
macro_rules! declare {
    ($e:expr, $t:ty) => {
        concord::api! {
            pub client Forwarded {
                base "http://x.test";
                POST broken(body p: pick!($e * 2) via Fmt);
            }
        }
        concord::api! {
            pub client ForwardedType {
                base "http://x.test";
                POST broken(body p: Vec<$t> via Fmt);
            }
        }
    };
}
declare!(1 + 2, u8);
fn main() {}
