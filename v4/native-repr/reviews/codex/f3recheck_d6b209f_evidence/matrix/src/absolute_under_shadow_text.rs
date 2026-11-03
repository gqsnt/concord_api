mod concord { pub use crate::codec::Text; }
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(::concord::Text b: String) -> ::concord::Text String;
        POST optional(::concord::Text b?: String);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(crate::c4::Text b: String) -> crate::c4::Text String;
        POST optional(crate::c4::Text b?: String);
    }
}
