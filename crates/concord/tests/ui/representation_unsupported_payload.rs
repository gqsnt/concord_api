// A format that does not support the declared payload type is reported at the statement's format,
// with the built-in choices: on the endpoint, on a scope and on the client. A built-in format
// that cannot carry a scalar is Concord's own error (not a `BodyFormat` mismatch).
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item { response text; }
        POST create(body item: Item) { request bytes; }
    }
}
concord::api! {
    pub client Scalar {
        base "http://x.test";
        POST count(body n: u64) { request text; }
        GET total() -> u64 { response bytes; }
    }
}
concord::api! {
    pub client Scoped {
        base "http://x.test";
        scope s {
            request text;
            response bytes;
            POST create(body n: u64) -> Item;
        }
    }
}
fn main() {}
