concord::api! {
    pub client Api {
        base "http://x.test";
        scope s(body b: String via concord::Text) { GET one(); }
    }
}
fn main() {}
