concord::api! {
    pub client Api {
        base "http://x.test";
        scope a(x: String) { path "{a.x}"; GET one(); }
    }
}
fn main() {}
