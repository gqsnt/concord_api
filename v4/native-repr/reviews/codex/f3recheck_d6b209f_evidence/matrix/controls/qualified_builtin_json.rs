crate::c4::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(concord::Json b: String) -> concord::Json String;
        POST optional(concord::Json b?: String);
    }
}
