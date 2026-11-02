type bytes = crate::codec::Binary;
crate::c4::api! {
    pub client Migrated {
        base "http://127.0.0.1";
        POST p(self::bytes b: Vec<u8>) -> self::bytes Vec<u8>;
        POST optional(self::bytes b?: Vec<u8>);
    }
}
crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(self::bytes b: Vec<u8>) -> self::bytes Vec<u8>;
        POST optional(self::bytes b?: Vec<u8>);
    }
}
