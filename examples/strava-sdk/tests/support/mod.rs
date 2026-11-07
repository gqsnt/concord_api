#![allow(dead_code)]
use concord_sdk_strava::{Credentials, Session, Strava, Tokens};
use std::time::Duration;
pub const ACTIVITY: &str = include_str!("../../fixtures/activity.json");
pub const UPDATED: &str = include_str!("../../fixtures/activity-updated.json");
pub const FIRST: &str = include_str!("../../fixtures/activities-1.json");
pub const SECOND: &str = include_str!("../../fixtures/activities-2.json");
pub const EMPTY: &str = include_str!("../../fixtures/activities-empty.json");
pub const RECEIPT: &str = include_str!("../../fixtures/upload-receipt.json");
pub const PENDING: &str = include_str!("../../fixtures/upload-pending.json");
pub const READY: &str = include_str!("../../fixtures/upload-ready.json");
pub const FAILED: &str = include_str!("../../fixtures/upload-failed.json");
pub const PROBLEM: &str = include_str!("../../fixtures/problem.json");
pub const INVALID: &str = include_str!("../../fixtures/invalid.json");
pub const TRACK: &[u8] = include_bytes!("../../fixtures/track.gpx");
pub const TOKENS: &str = include_str!("../../fixtures/tokens.json");
pub const EXPIRED: &str = include_str!("../../fixtures/expired-tokens.json");
pub const ROTATED: &str = include_str!("../../fixtures/rotated-tokens.json");
pub fn tokens(body: &str) -> Tokens {
    serde_json::from_str(body).unwrap()
}
pub fn credentials() -> Credentials {
    Credentials::new(123, "CLIENT_SECRET".into()).unwrap()
}
pub fn session(url: &str, token_body: &str, timeout: Duration) -> Session {
    Session::connect(url, url, credentials(), tokens(token_body), timeout).unwrap()
}
pub fn api(url: &str) -> Strava {
    session(url, TOKENS, Duration::from_secs(3)).client()
}
pub fn file() -> concord_sdk_strava::UploadFile {
    concord_sdk_strava::UploadFile::new(
        "synthetic.gpx",
        concord_sdk_strava::DataType::Gpx,
        concord_sdk_strava::concord::reqwest::Body::from(TRACK),
    )
    .unwrap()
    .external_id("synthetic-track".into())
}
pub fn safe(value: impl std::fmt::Debug) {
    let shown = format!("{value:?}");
    for secret in [
        "CLIENT_SECRET",
        "ACCESS_SECRET",
        "REFRESH_SECRET",
        "EXPIRED_SECRET",
        "CODE_SECRET",
        "BODY_SECRET",
        "UPLOAD_SECRET",
        "FILE_SECRET",
        "COOKIE_SECRET",
        "URL_SECRET",
    ] {
        assert!(!shown.contains(secret), "secret rendered in {shown}");
    }
}
pub fn safe_error(error: &concord_sdk_strava::Error) {
    safe(error);
    safe(format!("{error}"));
    assert!(std::error::Error::source(error).is_none());
}
