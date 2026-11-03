const MESSAGE: &str = "-> Item via Json";
// body b: String via Text is historical prose, not an active declaration.
concord::api! {
    pub client C {
        base "https://x.test";
        headers { "x-note" = "body a: String via Text"; }
        GET ready() -> text String;  GET old() -> Item via Json;
        POST raw(body r: String via Text) { path "r"; }
    }
}
