crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(crate::codec::Text b: String) -> crate::codec::Text String;
        POST optional(crate::codec::Text b?: String);
    }
}
