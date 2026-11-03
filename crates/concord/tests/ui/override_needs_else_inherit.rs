// An `override` reading an optional value must say what its absence means.
concord::api! {
    pub client Api(query locale: String = "en") {
        base "http://x.test";
        GET page(page_locale?: String) { query { override "locale" = page_locale; } }
    }
}
fn main() {}
