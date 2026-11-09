use std::rc::Rc;
concord::api! {
    client Api {
        base "http://127.0.0.1";
        GET page(query value: Rc<String>) { page next link; }
    }
}
fn across_threads(_: impl Future + Send) {}
fn main() {
    let endpoint = Api::new(concord::reqwest::Client::new()).page(Rc::new("local".into()));
    across_threads(endpoint.send());
}
