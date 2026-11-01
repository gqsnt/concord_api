concord::api! {
    pub client Api {
        base "http://x.test";
        scope a(x: String) { path "{x}"; GET one(); }
        scope b { GET two() { query { "x" = a.x; } } }
    }
}
fn main() {}
