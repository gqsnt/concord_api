concord::api! {
    pub client Api(lang?: String) {
        base "http://x.test";
        query { "lang" = lang; }
        GET set_lang();
        GET new();
        scope s { GET send(); }
    }
}
fn main() {}
