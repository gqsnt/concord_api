use crate::codec as concord;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(text b: String) -> text String;
        POST optional(text b?: String);
    }
}
