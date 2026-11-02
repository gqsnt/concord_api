
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via ::concord::Json) -> String via ::concord::Json;
        POST optional(body b?: String via ::concord::Json);
    }
}
