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

fn send_prepared<C: concord::page::Call>(call: C) {
    if let Ok(prepared) = call.prepare() {
        let _ = prepared.conditional();
    }
}

fn main() {
    let post = Api::new().create().edit_request(|request| {
        request.headers_mut().insert(
            "if-none-match",
            concord::http::HeaderValue::from_static("tag"),
        );
    });
    send_prepared(post);
}
