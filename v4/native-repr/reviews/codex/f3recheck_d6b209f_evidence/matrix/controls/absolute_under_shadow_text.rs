crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(crate::c4::Text b: String) -> crate::c4::Text String;
        POST optional(crate::c4::Text b?: String);
    }
}
