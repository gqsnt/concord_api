const MESSAGE: &str = "-> Json Item";
// Text b: String is historical prose, not an active declaration.
concord::api! {
    pub client C {
        base "https://x.test";
        headers { "x-note" = "Text a: String"; }
        GET ready() -> Json text String;  GET old() -> Item;
        POST raw(Text r: String) { path "r"; }
    }
}
