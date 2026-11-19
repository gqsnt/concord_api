//! An RFC 3339 reset: any value, counted from a fixed `Date`.
#![no_main]

use concord_core::http::{HeaderMap, HeaderName, HeaderValue};
use concord_core::reqwest::{self, Method, Request};
use concord_core::transport::TryInfo;
use concord_core::{fake, Middleware, Next};
use concord_extras::limit::{Bucket, Limit, Seen};
use libfuzzer_sys::fuzz_target;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

fn header(map: &mut HeaderMap, name: &'static str, value: &[u8]) {
    if let Ok(value) = HeaderValue::from_bytes(value) {
        map.append(HeaderName::from_static(name), value);
    }
}

/// Calls `look` with the `Seen` of one response with these headers, through a `Limit`.
fn seen<R: Send + 'static>(
    headers: HeaderMap,
    look: impl Fn(&Seen<'_>) -> R + Send + Sync + 'static,
) -> R {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a runtime")
    });
    let out = Arc::new(Mutex::new(None));
    let kept = out.clone();
    let limit = Limit::new().bucket(Bucket::new(|_| Some("k".into())).learn(move |seen| {
        *kept.lock().expect("not poisoned") = Some(look(seen));
        Vec::new()
    }));
    let transport = fake(move |_| {
        let mut response = concord_core::http::Response::new(reqwest::Body::from(""));
        *response.headers_mut() = headers.clone();
        Ok(response.into())
    });
    let request = Request::new(Method::GET, "https://one.test/data".parse().expect("a URL"));
    runtime
        .block_on(limit.handle(request, TryInfo::new("fuzz", 1), Next::to(&transport)))
        .expect("a limit never fails a received response");
    // The limit catches a panic of the learn function: nothing here is a panic to report.
    let result = out.lock().expect("not poisoned").take();
    result.expect("the learn function panicked")
}

fuzz_target!(|data: &[u8]| {
    let mut headers = HeaderMap::new();
    header(&mut headers, "date", b"Tue, 01 Jan 2030 00:00:00 GMT");
    header(&mut headers, "x-reset", data);
    let until = seen(headers, |seen| {
        seen.header("x-reset")
            .and_then(|text| seen.until_rfc3339(text))
    });
    if let Some(until) = until {
        assert!(until <= Duration::from_secs(1_577_847_600));
        assert!(data.len() <= 4096);
    }
});
