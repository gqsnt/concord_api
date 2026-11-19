//! An author's trait can use the name of the generated body helper (C10.1).
// PANIC-SAFETY: Unwraps make unexpected request construction failures fail the tests.
#![allow(clippy::unwrap_used)]

#[allow(dead_code)]
trait AuthorBody {
    fn whole_body<B>(&self, _: B) {}
}
impl<T> AuthorBody for T {}

concord::api! {
    client Requests {
        base "https://requests.test";
        POST expression(payload: concord::Body) { body payload; }
        POST entries(number: u8) { body concord::json { number; } }
    }
}

#[test]
fn c10_1_an_authored_whole_body_method_cannot_replace_a_body_expression() {
    let request = Requests::new()
        .expression(concord::Body::from("payload"))
        .request()
        .unwrap();
    assert_eq!(
        request.body().and_then(concord::reqwest::Body::as_bytes),
        Some(&b"payload"[..])
    );
}

#[test]
fn c10_1_an_authored_whole_body_method_cannot_replace_a_body_of_entries() {
    let request = Requests::new().entries(7u8).request().unwrap();
    assert_eq!(
        request.body().and_then(concord::reqwest::Body::as_bytes),
        Some(&br#"{"number":7}"#[..])
    );
}
