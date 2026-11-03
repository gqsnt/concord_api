facade::api! {
    pub client Expected {
        base "http://127.0.0.1";
        POST p(facade::Text b: String) -> facade::Text String;
        POST optional(facade::Text b?: String);
    }
}
