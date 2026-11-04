//! SDK version 3: `limit` becomes optional and initially present (`limit?: u32 = 20`) so that a
//! caller can omit it. Compatible for setter callers (`set_limit(5)` still compiles); the input
//! field's type changes from `u32` to `Option<u32>`.
concord::api! {
    pub client Shop(#[sensitive] header("x-api-key") key: String) {
        base "https://shop.example.test/v1";
        scope listing(query("tag") each default_tags: Vec<String> = Vec::new()) {
            GET list(
                query q?: String,
                tags?: Vec<String>,
                query limit?: u32 = 20,
                query sort?: String,
            ) {
                path "items";
                query { override "tag" = each tags else inherit; }
            }
        }
    }
}
