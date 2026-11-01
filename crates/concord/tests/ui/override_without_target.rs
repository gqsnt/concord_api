concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(page?: u32) { query { override "pgae" = page; } }
    }
}
fn main() {}
