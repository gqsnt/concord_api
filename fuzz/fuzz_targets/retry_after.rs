//! `Retry-After`: the header alone and the decision to retry read any value alike.
#![no_main]

use concord_core::__private::Idempotent;
use concord_core::error::Kind;
use concord_core::http::{HeaderMap, HeaderValue, Method, StatusCode};
use concord_core::retry::{again, Outcome};
use concord_core::{Headers, Retry};
use libfuzzer_sys::fuzz_target;
use std::time::{Duration, SystemTime};

fuzz_target!(|data: &[u8]| {
    // The first eight bytes are the clock, in seconds from 1970, before or after it.
    let (clock, value) = data.split_at(data.len().min(8));
    let mut seconds = [0u8; 8];
    seconds[..clock.len()].copy_from_slice(clock);
    let seconds = i64::from_le_bytes(seconds) % 253_402_300_800;
    let now = if seconds < 0 {
        SystemTime::UNIX_EPOCH.checked_sub(Duration::from_secs(seconds.unsigned_abs()))
    } else {
        SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds.unsigned_abs()))
    };
    let (Some(now), Ok(value)) = (now, HeaderValue::from_bytes(value)) else {
        return;
    };
    let mut map = HeaderMap::new();
    map.insert("retry-after", value);
    let headers = Headers::from(map);
    let read = headers.retry_after_at(now);
    let outcome = Outcome {
        kind: Kind::Refused,
        status: Some(StatusCode::TOO_MANY_REQUESTS),
        class: None,
        headers: Some(&headers),
        problem: None,
    };
    let decision = again(
        &Retry::<()>::standard(),
        outcome,
        1,
        &Method::GET,
        Idempotent::ByMethod,
        true,
        None,
        now,
        0.5,
    );
    assert_eq!(decision.header_wait, read);
});
