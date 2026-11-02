concord::api! {
    pub client Api {
        base "http://x.test";
        POST one(#[sensitive] body b: String via concord::Text);
    }
}
fn main() {}
