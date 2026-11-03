// 8g: a format's `Encode::CONTENT_TYPE` is validated when the declaration is compiled wherever it
// may be sent (here: a body without a declared media type), at the format.
pub struct Odd;
impl concord::Encode<String> for Odd {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x\u{2011}odd");
    fn encode(value: String) -> Result<concord::reqwest::Body, concord::CodecError> {
        Ok(value.into())
    }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        POST odd(body b: String) { request Odd; }
    }
}
fn main() {}
