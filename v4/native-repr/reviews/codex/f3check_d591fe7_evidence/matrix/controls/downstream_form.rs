crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(Form b: String) -> Form String;
        POST optional(Form b?: String);
    }
}
