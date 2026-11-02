// The V4-alpha spellings `-> Json<T>`, `Text`, `Binary` and `-> ()` get a precise replacement.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> concord::Json<Item>;
    }
}
concord::api! {
    pub client Unit {
        base "http://x.test";
        DELETE remove() -> ();
    }
}
fn main() {}
