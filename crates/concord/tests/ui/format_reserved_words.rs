// Format words are reserved in format position; a lowercase format type is written as a path.
pub struct Rows;
concord::api! {
    pub client MissingType {
        base "http://x.test";
        GET item() -> json { path "x"; }
    }
}
concord::api! {
    pub client Lowercase {
        base "http://x.test";
        GET rows() -> csv Rows;
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
fn main() {}
