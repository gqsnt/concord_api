#[cfg(test)]
use sdk_or_concord::reqwest;
use sdk_or_concord::{Binary, Form, IgnoreContentType, Json, Text};
use std::collections::BTreeMap;
type Adapter<F> = IgnoreContentType<F>;
sdk_or_concord::api! {
    pub client Api {
        base "http://127.0.0.1:9";
        POST json(body value: u32) -> u32 { path "json"; format Json; }
        POST text(body value: String) -> String { path "text"; format Text; }
        POST binary(body value: Vec<u8>) -> Vec<u8> { path "binary"; format Binary; }
        POST form(body value: BTreeMap<String, String>) -> BTreeMap<String, String> { path "form"; format Form; }
        POST ignored(body value: u32) -> u32 { path "ignored"; format IgnoreContentType<Json>; }
        POST absolute(body value: u32) -> u32 { path "absolute"; format ::sdk_or_concord::Json; }
        POST generic(body value: u32) -> u32 { path "generic"; format Adapter<Json>; }
    }
}
#[test]
fn json_paths_and_adapters() {
    let api = Api::new(reqwest::Client::new());
    for p in [
        api.json(7).prepare().unwrap(),
        api.ignored(7).prepare().unwrap(),
        api.absolute(7).prepare().unwrap(),
        api.generic(7).prepare().unwrap(),
    ] {
        assert_eq!(p.request().headers()["content-type"], "application/json");
        assert_eq!(p.request().headers()["accept"], "application/json");
        assert_eq!(p.request().body().unwrap().as_bytes(), Some(&b"7"[..]));
    }
}
#[test]
fn text_and_binary_requests() {
    let api = Api::new(reqwest::Client::new());
    let p = api.text("hello").prepare().unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert_eq!(p.request().body().unwrap().as_bytes(), Some(&b"hello"[..]));
    assert!(!p.request().headers().contains_key("accept"));
    let p = api.binary(vec![0, 255]).prepare().unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "application/octet-stream"
    );
    assert_eq!(p.request().body().unwrap().as_bytes(), Some(&[0, 255][..]));
}
#[test]
fn form_request() {
    let api = Api::new(reqwest::Client::new());
    let p = api
        .form(BTreeMap::from([("key".into(), "a b".into())]))
        .prepare()
        .unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(
        p.request().headers()["accept"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(&b"key=a+b"[..])
    );
}
