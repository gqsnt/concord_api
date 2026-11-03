// `via` was removed; the error names the exact replacement, in place or under a default.
pub struct Item;
pub struct Csv;
concord::api! {
    pub client Output {
        base "http://x.test";
        GET item() -> Item via concord::Json;
    }
}
concord::api! {
    pub client Custom {
        base "http://x.test";
        GET rows() -> Vec<Item> via Csv;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        PUT note(body note?: String via Text);
    }
}
concord::api! {
    pub client NoBodyKeyword {
        base "http://x.test";
        PUT upload(data: Vec<u8> via concord::Binary);
    }
}
fn main() {}
