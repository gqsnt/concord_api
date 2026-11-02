// An inherited format that cannot carry an endpoint's payload is reported at that endpoint's
// payload type (not at the distant default), with the trait context.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        request bytes;
        response text;
        GET item() -> Item;
        POST create(body item: Item);
    }
}
fn main() {}
