type bytes = crate::codec::Binary;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(bytes b: Vec<u8>) -> bytes Vec<u8>;
        POST optional(bytes b?: Vec<u8>);
    }
}
