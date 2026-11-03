concord::api! {
    pub client Api {
        base "http://x.test";
        headers { "X-Tag" = "a"; }
        GET list() { headers { "x-tag" = "b"; } }
    }
}
fn main() {}
