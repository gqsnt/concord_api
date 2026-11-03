use reqwest::header::HeaderName;
use sdk::{CodecError, Decode, Encode, ResponseHead, bytes::Bytes, reqwest};
pub const NATIVE: HeaderName = HeaderName::from_static("x-consumer");
pub struct MyJson;
#[allow(non_camel_case_types)]
type json = MyJson;
pub struct ReadOnly;
pub struct WriteOnly;
impl Encode<String> for MyJson {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-downstream");
    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(value.into())
    }
}
impl Decode<String> for MyJson {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/x-downstream");
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<String, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("invalid text"))
    }
}
impl Decode<String> for ReadOnly {
    type Context = ();
    const ACCEPT: Option<&'static str> = Some("application/x-read-only");
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<String, CodecError> {
        MyJson::decode_body((), body)
    }
}
impl Encode<String> for WriteOnly {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x-write-only");
    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(value.into())
    }
}
sdk::api! {
    pub client Api {
        base "http://127.0.0.1:9";
        format json;
        headers { NATIVE = "isolated"; }
        POST both(body value: String) -> String { path "both"; }
        GET read() -> String { response ReadOnly; path "read"; }
        POST write(body value: String) { request WriteOnly; path "write"; }
        #[cfg(any())]
        GET disabled() -> String { response NonexistentFormat; path "disabled"; }
    }
}
#[test]
fn renamed_facade_and_lowercase_codec() {
    let api = Api::new(reqwest::Client::new());
    let p = api.both("payload").prepare().unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "application/x-downstream"
    );
    assert_eq!(p.request().headers()["accept"], "application/x-downstream");
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(&b"payload"[..])
    );
}
#[test]
fn one_direction_formats() {
    let api = Api::new(reqwest::Client::new());
    let p = api.read().prepare().unwrap();
    assert_eq!(p.request().headers()["accept"], "application/x-read-only");
    let p = api.write("write").prepare().unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "application/x-write-only"
    );
    assert!(!p.request().headers().contains_key("accept"));
}
#[test]
fn native_header_validation() {
    Api::validate_header_names().unwrap();
    let p = Api::new(reqwest::Client::new()).read().prepare().unwrap();
    assert_eq!(p.request().headers()[NATIVE], "isolated");
}
