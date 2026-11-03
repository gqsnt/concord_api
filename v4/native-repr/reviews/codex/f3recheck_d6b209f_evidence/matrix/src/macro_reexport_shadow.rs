mod concord { pub use crate::c4::api; pub use crate::codec::Json; }
concord::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(concord::Json b: String) -> concord::Json String;
        POST optional(concord::Json b?: String);
    }
}
concord::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(concord::Json b: String) -> concord::Json String;
        POST optional(concord::Json b?: String);
    }
}
