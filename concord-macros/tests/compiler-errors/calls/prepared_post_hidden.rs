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
use concord::__private::Call as _;
fn main() {
    let post = Api::new().create().edit_request(|request| {
        request.headers_mut().insert(
            "if-none-match",
            concord::http::HeaderValue::from_static("tag"),
        );
    });
    let _ = post.prepare().unwrap().conditional();
}
