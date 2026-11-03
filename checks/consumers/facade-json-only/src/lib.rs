//! With only `json` enabled, `Form` is absent from the facade:
//!
//! ```compile_fail,E0432
//! use sdk::Form;
//! ```
#[cfg(test)]
use sdk::reqwest;
use sdk::{Json, Text};
sdk::api! {
    pub client Api {
        base "http://127.0.0.1:9";
        POST json(body value: u32) -> u32 { path "json"; format Json; }
        POST text(body value: String) -> String { path "text"; format Text; }
    }
}
#[test]
fn json_available_without_reqwest_defaults() {
    let p = Api::new(reqwest::Client::new()).json(7).prepare().unwrap();
    assert_eq!(p.request().headers()["content-type"], "application/json");
    assert_eq!(p.request().headers()["accept"], "application/json");
    assert_eq!(p.request().body().unwrap().as_bytes(), Some(&b"7"[..]));
}
#[test]
fn text_is_also_available() {
    let p = Api::new(reqwest::Client::new())
        .text("hello")
        .prepare()
        .unwrap();
    assert_eq!(
        p.request().headers()["content-type"],
        "text/plain; charset=utf-8"
    );
    assert_eq!(p.request().body().unwrap().as_bytes(), Some(&b"hello"[..]));
}
