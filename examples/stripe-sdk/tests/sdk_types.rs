//! SDK values carry wire spelling and secrecy independently.

use concord_example_stripe_sdk::{AccountId, IdempotencyKey, SecretKey, send::ApiError};

#[test]
fn a_secret_wire_value_retains_its_base_literal_and_redacts_both_debug_modes() {
    let key = IdempotencyKey::new("application-key/1+%").unwrap();
    assert_eq!(key.to_string(), "application-key/1+%");
    assert_eq!(format!("{key:>40}"), "application-key/1+%");
    assert_eq!(
        serde_json::to_string(&key).unwrap(),
        "\"application-key/1+%\""
    );
    assert_eq!(format!("{key:?}"), "IdempotencyKey(<redacted>)");
    assert_eq!(format!("{key:#?}"), "IdempotencyKey(\n    <redacted>,\n)");
    let account = AccountId::new("acct_SECRET").unwrap();
    let secret = SecretKey::new("sk_test_SECRET").unwrap();
    assert_eq!(format!("{account:?}"), "AccountId(<redacted>)");
    assert_eq!(format!("{account:#?}"), "AccountId(\n    <redacted>,\n)");
    assert_eq!(format!("{secret:?}"), "SecretKey(<redacted>)");
    assert_eq!(format!("{secret:#?}"), "SecretKey(\n    <redacted>,\n)");
}

#[test]
fn provider_error_fields_withhold_contents_and_keep_optional_presence() {
    let error: ApiError = serde_json::from_value(serde_json::json!({
        "type": "api_error", "code": "test", "message": "MODEL_SECRET",
        "param": "MODEL_SECRET", "doc_url": "https://MODEL_SECRET.invalid"
    }))
    .unwrap();
    assert_eq!(
        format!("{error:?}"),
        "ApiError { kind: ApiError, code: Some(\"test\"), message: Some(<redacted>), param: Some(<redacted>), doc_url: Some(<redacted>) }"
    );
    assert!(!format!("{error:#?}").contains("MODEL_SECRET"));
    assert_eq!(error.message(), Some("MODEL_SECRET"));
    assert_eq!(error.param(), Some("MODEL_SECRET"));
    assert_eq!(
        error.doc_url.as_deref(),
        Some("https://MODEL_SECRET.invalid")
    );
}
