// A statement without a format, and a misspelled placement, are Concord's errors. (Unknown format
// names are Rust's own errors: `format_unknown_type.rs`.)
pub struct Rows;
concord::api! {
    pub client MissingFormat {
        base "http://x.test";
        GET item() -> Rows { response; }
    }
}
concord::api! {
    pub client Typo {
        base "http://x.test";
        GET search(qurey page: u32);
    }
}
fn main() {}
