// F03 (4.2): migration advice renders the payload type from its tokens; a literal inside it keeps
// its exact contents (`"a :: b"`, six bytes), so applying the advice keeps the meaning
// (tests/applied_advice.rs compiles the applied text).
use concord::{CodecError, Encode, reqwest};
pub const fn count(s: &str) -> usize { s.len() }
pub struct Payload<const N: usize>;
pub struct Fmt;
impl<const N: usize> Encode<Payload<N>> for Fmt {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(_: Payload<N>) -> Result<reqwest::Body, CodecError> { Ok(reqwest::Body::from(N.to_string())) }
}
concord::api! {
    pub client ViaLiteral {
        base "http://x.test";
        POST broken(body p: Payload<{ count("a :: b") }> via Fmt);
    }
}
concord::api! {
    pub client TrailingFormatLiteral {
        base "http://x.test";
        POST broken(p: Payload<{ count(r#"x , y"#) }> Fmt);
    }
}
fn main() {}
