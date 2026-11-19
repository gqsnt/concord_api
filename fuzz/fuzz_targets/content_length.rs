//! `Content-Length`: any value, announced on a body of up to 255 bytes read under any limit.
#![no_main]

use concord_core::bytes::Bytes;
use concord_core::http::{HeaderValue, Response};
use concord_core::read::Incoming;
use futures_util::FutureExt as _;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // The limit, the length of the body, then the value of the header.
    let [limit, size, value @ ..] = data else {
        return;
    };
    let Ok(value) = HeaderValue::from_bytes(value) else {
        return;
    };
    let (limit, size) = (usize::from(*limit), usize::from(*size));
    let body = vec![Ok::<_, std::io::Error>(Bytes::from(vec![b'x'; size]))];
    let mut response = Response::new(concord_core::reqwest::Body::wrap_stream(
        futures_util::stream::iter(body),
    ));
    response
        .headers_mut()
        .insert("content-length", value.clone());
    let read = Incoming::new(concord_core::reqwest::Response::from(response))
        .bytes(limit)
        .now_or_never()
        .expect("a body in memory is read at once");
    // Only digits announce a size, when they fit in `u64`.
    let announced = std::str::from_utf8(value.as_bytes())
        .ok()
        .filter(|text| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .and_then(|text| text.parse::<u64>().ok());
    let fits = size <= limit;
    match announced {
        // Nothing announced: the body is read whole when it is within the limit.
        None => assert_eq!(read.map(|bytes| bytes.len()).ok(), fits.then_some(size)),
        // Over the limit, or more than came: an error. Exactly what came: the body.
        Some(length) if length > limit as u64 || length > size as u64 => assert!(read.is_err()),
        Some(length) if length == size as u64 && fits => assert_eq!(read.unwrap().len(), size),
        Some(_) => {}
    }
});
