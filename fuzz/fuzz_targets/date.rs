//! `Date`: any value, as a limit reads it for an RFC 3339 reset, against `Retry-After`.
#![no_main]

use concord_core::http::{HeaderMap, HeaderName, HeaderValue};
use concord_core::reqwest::{self, Method, Request};
use concord_core::transport::TryInfo;
use concord_core::Headers;
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
    let mut with_date = HeaderMap::new();
    header(&mut with_date, "date", data);
    // A reset in 2030, from the response's `Date` and from this clock.
    let reset = "2030-01-01T00:00:00Z";
    let (dated, now) = seen(with_date, move |seen| {
        (seen.until_rfc3339(reset), seen.now())
    });
    let local = seen(HeaderMap::new(), move |seen| {
        (seen.until_rfc3339(reset), seen.now())
    });
    if let Some(until) = dated {
        assert!(until <= Duration::from_secs(1_577_847_600));
    }
    let moved = match (dated, local.0) {
        (Some(a), Some(b)) => a.abs_diff(b) > Duration::from_secs(5),
        (a, b) => a.is_some() != b.is_some(),
    };
    // A `Date` that moved the reset is an HTTP date, which `Retry-After` reads too.
    if moved {
        let mut map = HeaderMap::new();
        map.insert(
            "retry-after",
            HeaderValue::from_bytes(data).expect("a header value"),
        );
        assert!(Headers::from(map).retry_after_at(now).is_some());
    }
});
