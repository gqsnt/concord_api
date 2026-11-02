// The V4-alpha spellings `-> Json<T>` and `body b: Json<T>` are ordinary payload types now: without
// a `request` / `response` statement they get the "no representation" error (the dedicated
// alpha hints went away with the in-place format). A typed unit `-> ()` needs a representation
// as well: omit `->` to drain.
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> concord::Json<Item>;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        POST create(body item: Json<Item>);
    }
}
concord::api! {
    pub client Unit {
        base "http://x.test";
        DELETE remove() -> ();
    }
}
fn main() {}
