concord::api! {
    pub client Api {
        base "http://x.test";
        POST one(#[sensitive] body b: String) { request concord::Text; }
    }
}
fn main() {}
