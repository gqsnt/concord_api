// Every body and typed output has a format, written in place or inherited from the nearest
// `request`/`response` default: without either it is an error at the payload, naming both. The
// two directions are independent: a `request` default does not give outputs a format.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        request json;
        GET item() -> Item;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        response json;
        POST create(body item: Item);
    }
}
fn main() {}
