// Review of F03 (4.2): advice keeps punctuation written apart (`choose!(. .)` is not
// `choose!(..)`, another macro arm) and the grouping of a fragment forwarded by `macro_rules!`
// (`(1 + 2) * 2`, not `1 + 2 * 2`). tests/applied_advice.rs compiles the applied text.
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
}
macro_rules! pick { ($e:expr) => { Payload<{ $e }> }; }
concord::api! {
    pub client Separate {
        base "http://x.test";
        POST broken(body p: choose!(. .) via Fmt);
    }
}
macro_rules! declare {
    ($e:expr) => {
        concord::api! {
            pub client Forwarded {
                base "http://x.test";
                POST broken(body p: pick!($e * 2) via Fmt);
            }
        }
    };
}
declare!(1 + 2);
fn main() {}
