// `else inherit` on a source that is always present never applies, so it is rejected.
concord::api! {
    pub client Api(query locale: String = "en") {
        base "http://x.test";
        GET page(page_locale: String) { query { override "locale" = page_locale else inherit; } }
    }
}
fn main() {}
