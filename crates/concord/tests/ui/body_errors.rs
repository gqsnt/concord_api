concord::api! {
    pub client Api {
        base "http://x.test";
        scope s(body b: concord::Text) { GET one(); }
    }
}
fn main() {}
