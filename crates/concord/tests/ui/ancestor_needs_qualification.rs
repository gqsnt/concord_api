concord::api! {
    pub client Api(locale: String = "en") {
        base "http://x.test";
        query { "l" = locale; }
        GET item() { query { "locale" = locale; } }
    }
}
fn main() {}
