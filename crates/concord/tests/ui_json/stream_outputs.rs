// A stream with no format anywhere.
concord::api! {
    pub client Formatless {
        base "http://x.test";
        GET events() -> stream String;
    }
}
// `-> stream` without an item takes no format statement.
concord::api! {
    pub client Itemless {
        base "http://x.test";
        GET export() -> stream { response concord::Json; }
    }
}
// A format is a statement, not a second type.
concord::api! {
    pub client TwoTypes {
        base "http://x.test";
        GET events() -> stream String concord::Json;
    }
}
fn main() {}
