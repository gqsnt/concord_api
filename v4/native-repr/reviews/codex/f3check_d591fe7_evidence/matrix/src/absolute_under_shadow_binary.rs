mod concord { pub use crate::codec::Binary; }
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(bytes b: Vec<u8>) -> bytes Vec<u8>;
        POST optional(bytes b?: Vec<u8>);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(crate::c4::Binary b: Vec<u8>) -> crate::c4::Binary Vec<u8>;
        POST optional(crate::c4::Binary b?: Vec<u8>);
    }
}
