use std::error::Error;

use concord_core::advanced::HookError;

const SAFE_MESSAGE: &str = "application-approved lifecycle veto";
const BODY_SENTINEL: &str = "hook-body-sentinel-771bca";
const NATIVE_URL_SENTINEL: &str = "hook-native-url-sentinel-418fae";

#[test]
fn hook_error_formats_only_application_owned_safe_message() {
    let error = HookError::new(SAFE_MESSAGE);
    let display = error.to_string();
    let debug = format!("{error:?}");
    assert!(display.contains(SAFE_MESSAGE));
    assert!(debug.contains(SAFE_MESSAGE));
    for forbidden in [BODY_SENTINEL, NATIVE_URL_SENTINEL] {
        assert!(!display.contains(forbidden));
        assert!(!debug.contains(forbidden));
    }
    assert!(Error::source(&error).is_none());
}
