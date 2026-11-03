crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(super::Text b: String) -> super::Text String;
        POST optional(super::Text b?: String);
    }
}
