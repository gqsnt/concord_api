
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(super::codec::Text b: String) -> super::codec::Text String;
        POST optional(super::codec::Text b?: String);
    }
}
