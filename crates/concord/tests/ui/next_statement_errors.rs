// `page next` is one statement on a bodyless `GET` endpoint; `url` names a typed output's field.
concord::api! {
    pub client Posted {
        base "http://x.test";
        POST create(body text: String) -> String { request concord::Text; response concord::Text; page next link; }
    }
}
concord::api! {
    pub client OnScope {
        base "http://x.test";
        scope s { page next link; GET one(); }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        GET list() -> String { response concord::Text; page next link; page next url next; }
    }
}
concord::api! {
    pub client Misspelled {
        base "http://x.test";
        GET list() -> String { response concord::Text; page next links; }
    }
}
concord::api! {
    pub client NoOutput {
        base "http://x.test";
        GET list() { page next url next; }
    }
}

fn main() {}
