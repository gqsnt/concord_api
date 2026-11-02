use concord::Json;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(Json b: String) -> Json String;
        POST optional(Json b?: String);
    }
}
