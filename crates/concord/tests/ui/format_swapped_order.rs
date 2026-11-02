// The format comes first, on outputs and on bodies.
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
