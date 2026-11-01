concord::api! {
    pub client Api {
        base "http://x.test";
        POST one(#[sensitive] body b: concord::Text);
    }
}
fn main() {}
