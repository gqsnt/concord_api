concord::api! {
    pub client Api {
        base "http://x.test";
        scope s(body b: String) { GET one(); }
    }
}
concord::api! {
    pub client Inherited {
        base "http://x.test";
        request json;
        scope s(body b: String) { GET one(); }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        POST two(body a: String, body b: Vec<u8>) { request text; }
    }
}
concord::api! {
    pub client Defaulted {
        base "http://x.test";
        request json;
        POST one(body b: String = String::new());
    }
}
fn main() {}
