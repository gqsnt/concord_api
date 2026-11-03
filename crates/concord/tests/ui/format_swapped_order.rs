// A format written after the payload type (the swapped order of the removed in-place spelling) is
// reported like the removed spelling itself: the message names the `request` / `response` statement.
pub struct Item;
concord::api! {
    pub client Output {
        base "http://x.test";
        GET item() -> Vec<Item> json;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        POST create(item: Item json);
    }
}
fn main() {}
