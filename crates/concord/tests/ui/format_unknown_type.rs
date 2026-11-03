// A format is an ordinary type path: no word is reserved, so an unknown name (lowercase or not, a
// misspelling, the old `json` word) is Rust's own unresolved-type error, at the name as written.
// (A forgotten import: `ui_json/format_missing_import.rs`.)
pub struct Rows;

pub struct Item;

concord::api! {
    pub client Lowercase {
        base "http://x.test";
        GET rows() -> Rows { response csv; }
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
        POST create(body item: Rows) { format jsn; }
    }
}

// The old word: an ordinary (unknown) lowercase type now.
concord::api! {
    pub client OldWord {
        base "http://x.test";
        POST create(body item: Item) { request json; }
    }
}

fn main() {}
