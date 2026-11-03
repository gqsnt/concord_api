// Every body and typed output has a format, from the nearest `request`/`response` statement (the
// endpoint, a scope or the client): without one it is an error at the payload. The two directions
// are independent: a `request` statement does not give outputs a format.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        request concord::Json;
        GET item() -> Item;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        response concord::Json;
        POST create(body item: Item);
    }
}
fn main() {}
