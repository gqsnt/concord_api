
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
        POST p(crate::c4::Json b: String) -> crate::c4::Json String;
        POST optional(crate::c4::Json b?: String);
    }
}
