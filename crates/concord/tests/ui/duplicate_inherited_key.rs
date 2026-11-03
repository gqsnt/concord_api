concord::api! {
    pub client Api {
        base "http://x.test";
        query { "page" = "1"; }
        scope s {
            GET list(page: u32) { query { "page" = page; } }
        }
    }
}
fn main() {}
