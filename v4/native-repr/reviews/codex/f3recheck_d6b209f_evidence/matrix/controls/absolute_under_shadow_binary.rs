crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(crate::c4::Binary b: Vec<u8>) -> crate::c4::Binary Vec<u8>;
        POST optional(crate::c4::Binary b?: Vec<u8>);
    }
}
