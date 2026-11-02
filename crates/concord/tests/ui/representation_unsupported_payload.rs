// A format that does not support the declared payload type is reported at the format word, with
// the built-in choices.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> text Item;
        POST create(bytes item: Item);
    }
}
fn main() {}
