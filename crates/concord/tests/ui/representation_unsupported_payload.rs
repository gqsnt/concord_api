// A representation that does not support the declared payload type is reported at the
// representation, with the built-in choices.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item via concord::Text;
        POST create(body item: Item via concord::Binary);
    }
}
fn main() {}
