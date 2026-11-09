use std::rc::Rc;
use concord::{CodecError, Decode, Endpoint, Follow, NextUrl, ResponseHead, bytes::Bytes};
struct Local;
impl Decode<Rc<String>> for Local {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> { Ok(()) }
    fn decode_body(_: (), _: Bytes) -> Result<Rc<String>, CodecError> { Ok(Rc::new(String::new())) }
}
concord::api! {
    client Api {
        base "http://127.0.0.1";
        GET page(query value: Rc<String>) -> Rc<String> { response Local; page next link; }
        GET output() -> Rc<String> { response Local; page next link; }
        GET stream(query value: Rc<String>) -> stream { page next link; }
    }
}
fn send_future(_: impl Future + Send) {}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let target = || NextUrl::parse("http://127.0.0.1/page?opaque=1").unwrap();
    let prepared = api.page(Rc::new("local".into())).follow(target()).prepare().unwrap();
    assert_eq!(prepared.operation().unwrap(), ApiPage::operation());
    // Local inputs remain usable with the original local send future too.
    let _local = api.page(Rc::new("local".into())).send();
    let _stream = api.stream(Rc::new("local".into())).follow(target()).prepare().unwrap();
    // The future may move between threads even when its final output is local.
    send_future(api.output().follow(target()).send());
}
