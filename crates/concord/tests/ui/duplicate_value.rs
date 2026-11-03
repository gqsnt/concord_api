concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(a: u32, a: u32) { query { "a" = a; } }
    }
}
fn main() {}
