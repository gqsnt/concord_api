// Format words are reserved in a representation statement; a lowercase format type is written as
// a path. A statement without a format is an error, and so is a typo.
pub struct Rows;
concord::api! {
    pub client MissingFormat {
        base "http://x.test";
        GET item() -> Rows { response; }
    }
}
concord::api! {
    pub client Lowercase {
        base "http://x.test";
        GET rows() -> Rows { response csv; }
    }
}
concord::api! {
    pub client Typo {
        base "http://x.test";
        GET search(qurey page: u32);
    }
}
concord::api! {
    pub client DefaultTypo {
        base "http://x.test";
        request jsn;
        POST create(body item: Rows);
    }
}
concord::api! {
    pub client EndpointTypo {
        base "http://x.test";
        POST create(body item: Rows) { request jsn; }
    }
}
concord::api! {
    pub client FormatTypo {
        base "http://x.test";
        POST create(body item: Rows) { format jsn; }
    }
}
fn main() {}
