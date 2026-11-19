//! `Content-Length`: any value, announced on an empty body read under any limit.
#![no_main]

use concord_core::bytes::Bytes;
use concord_core::http::{HeaderValue, Response};
use concord_core::read::Incoming;
use futures_util::FutureExt as _;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((&limit, value)) = data.split_first() else {
        return;
    };
    let Ok(value) = HeaderValue::from_bytes(value) else {
        return;
    };
    let source = futures_util::stream::iter(Vec::<Result<Bytes, std::io::Error>>::new());
    let mut response = Response::new(concord_core::reqwest::Body::wrap_stream(source));
    response
        .headers_mut()
        .insert("content-length", value.clone());
    let read = Incoming::new(concord_core::reqwest::Response::from(response))
        .bytes(usize::from(limit))
        .now_or_never()
        .expect("an empty body is read at once");
    // Only digits announce a size, when they fit in `u64`; an empty body under any other
    // value is read whole.
    let announced = std::str::from_utf8(value.as_bytes())
        .ok()
        .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok());
    match announced {
        None => assert!(read.expect("nothing announced").is_empty()),
        Some(0) => assert!(read.is_ok()),
        Some(_) => assert!(read.is_err()),
    }
});
