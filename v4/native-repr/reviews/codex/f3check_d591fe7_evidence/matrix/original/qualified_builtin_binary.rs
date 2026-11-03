
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: Vec<u8> via concord::Binary) -> Vec<u8> via concord::Binary;
        POST optional(body b?: Vec<u8> via concord::Binary);
    }
}
