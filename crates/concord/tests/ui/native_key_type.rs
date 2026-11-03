// A header key path must name a native `HeaderName`; a string constant is not converted (H08),
// and query keys stay quoted strings.
const NAME: &str = "x-token";

concord::api! {
    pub client Api(header(NAME) token: String) {
        base "http://x.test";
        GET get();
    }
}

concord::api! {
    pub client Query(query(NAME) q: String) {
        base "http://x.test";
        GET get();
    }
}

fn main() {}
