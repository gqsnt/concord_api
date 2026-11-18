#![allow(dead_code)]
use concord::{by_offset, Json};
concord::api! {
    pub client Api {
        base "https://calls.test/";
        POST create();
        GET get();
        GET list(start?: u32) -> Vec<u32> {
            query { start; }
            read Json;
            paginate by_offset(start);
        }
    }
}
struct Added;
impl api::Endpoint for Added {
    type Value = ();
}
fn main() {}
