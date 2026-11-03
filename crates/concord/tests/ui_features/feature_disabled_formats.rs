// 12a/12b with the `json` and `form` features off: `Json` and `Form` are ordinary items gated by
// those features, so importing them fails where they are imported, and rustc names the feature.
// (A format named only by cfg-disabled endpoints or by unused defaults is never resolved:
// `tests/attributes.rs`.)
use concord::{Form, Json};
pub struct Item;
concord::api! {
    pub client InPlace {
        base "http://x.test";
        GET a() -> Item { response Json; }
        POST login(body f: Item) { request Form; }
    }
}
fn main() {}
