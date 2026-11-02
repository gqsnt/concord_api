// The representation is part of the declaration: `-> T via R`. No default, no inference.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item;
    }
}
fn main() {}
