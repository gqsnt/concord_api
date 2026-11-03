mod nested { pub mod concord { pub use crate::codec::Json; } }
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(nested::concord::Json b: String) -> nested::concord::Json String;
        POST optional(nested::concord::Json b?: String);
    }
}
