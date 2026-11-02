concord::api! {
    pub client Api {
        base "http://x.test";
        POST one(#[sensitive] text b: String);
    }
}
fn main() {}
