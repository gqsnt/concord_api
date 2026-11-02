crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(Binary b: Vec<u8>) -> Binary Vec<u8>;
        POST optional(Binary b?: Vec<u8>);
    }
}
