//! SDK version 1.
concord::api! {
    pub client Shop(#[sensitive] header("x-api-key") key: String) {
        base "https://shop.example.test/v1";
        scope listing(query("tag") each default_tags: Vec<String> = Vec::new()) {
            GET list(query q?: String, tags?: Vec<String>, query limit: u32 = 20) {
                path "items";
                query { override "tag" = each tags else inherit; }
            }
        }
    }
}
