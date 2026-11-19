//! `Json.strict()`: any content type, before a body that is JSON.
#![no_main]

use concord_core::__private::read_body;
use concord_core::http::{HeaderValue, Response};
use concord_core::transport::TryInfo;
use concord_core::Error;
use concord_extras::read::Json;
use futures_util::FutureExt as _;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(label) = HeaderValue::from_bytes(data) else {
        return;
    };
    let mut response = Response::new(concord_core::reqwest::Body::from("[1]"));
    response.headers_mut().insert("content-type", label.clone());
    let read: Result<Vec<u8>, Error> = read_body(
        &Json.strict(),
        concord_core::reqwest::Response::from(response),
        &TryInfo::new("fuzz.label", 1),
    )
    .now_or_never()
    .expect("a body in memory is read at once");
    if read.is_ok() {
        let text = label
            .to_str()
            .expect("a JSON content type is text")
            .to_ascii_lowercase();
        let essence = text.split(';').next().unwrap_or_default().trim();
        assert!(essence == "application/json" || essence.ends_with("+json"));
    }
});
