
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(json b: String) -> json String;
        POST optional(json b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(concord::Json b: String) -> concord::Json String;
        POST optional(concord::Json b?: String);
    }
}
