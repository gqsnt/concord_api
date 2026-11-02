mod nested { pub mod concord { pub use crate::codec::Json; } }
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: String via nested::concord::Json) -> String via nested::concord::Json;
        POST optional(body b?: String via nested::concord::Json);
    }
}
