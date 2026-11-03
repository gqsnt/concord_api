use concord::Binary;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(body b: Vec<u8> via Binary) -> Vec<u8> via Binary;
        POST optional(body b?: Vec<u8> via Binary);
    }
}
