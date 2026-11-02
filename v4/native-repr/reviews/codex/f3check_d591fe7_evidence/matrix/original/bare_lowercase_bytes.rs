type bytes = crate::codec::Binary;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: Vec<u8> via bytes) -> Vec<u8> via bytes;
        POST optional(body b?: Vec<u8> via bytes);
    }
}
