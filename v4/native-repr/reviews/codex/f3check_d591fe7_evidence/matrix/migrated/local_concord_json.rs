mod concord { pub use crate::codec::Json; }
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(json b: String) -> json String;
        POST optional(json b?: String);
    }
}
