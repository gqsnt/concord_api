use std::error::Error;

use concord_core::contracts::{ConfigurationErrorKind, SafeProxy};

const PROXY_SENTINEL: &str = "proxy-user-password-sentinel-430f5a";

#[test]
fn credential_bearing_proxy_error_is_structured_and_sentinel_free() {
    let value = format!("http://user:{PROXY_SENTINEL}@proxy.example");
    let error = match SafeProxy::parse(value) {
        Ok(_) => panic!("credential-bearing proxy unexpectedly parsed"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ConfigurationErrorKind::InvalidProxy);
    assert!(!error.to_string().contains(PROXY_SENTINEL));
    assert!(!format!("{error:?}").contains(PROXY_SENTINEL));
    assert!(error.source().is_none());
}
