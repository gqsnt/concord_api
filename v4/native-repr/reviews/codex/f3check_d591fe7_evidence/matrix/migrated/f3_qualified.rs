use crate::codec;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(codec::Text b: String) -> codec::Text String;
        POST optional(codec::Text b?: String);
    }
}
