concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(tags: Vec<String>) { query { "tag" = each tags; "all" = tags; } }
    }
}
fn main() {}
