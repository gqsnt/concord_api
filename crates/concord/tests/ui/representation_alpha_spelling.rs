// The V4-alpha spellings `-> Json<T>`, `body b: Json<T>`, `Text`, `Binary` and `-> ()` get a
// precise replacement. The spelling only words the error: under a default the same payload types
// would be ordinary (and fail as types).
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
