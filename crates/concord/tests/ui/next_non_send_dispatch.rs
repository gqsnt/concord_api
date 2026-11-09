use std::rc::Rc;
use concord::{Follow, NextUrl};
concord::api! {
    client Api {
        base "http://127.0.0.1";
        GET page(query value: Rc<String>) { page next link; }
    }
}
fn main() {
    let endpoint = Api::new(concord::reqwest::Client::new()).page(Rc::new("local".into()));
    let _future = endpoint.follow(NextUrl::parse("http://127.0.0.1/page").unwrap()).send();
}
