//! Frozen scalar spellings from the pre-migration formatter.
use concord_sdk_anthropic_messages::ApiErrorType;

#[test]
fn scalar_error_codes_keep_every_wire_literal() {
    for (value, expected) in [
        (ApiErrorType::InvalidRequest, "invalid_request_error"),
        (ApiErrorType::Authentication, "authentication_error"),
        (ApiErrorType::Billing, "billing_error"),
        (ApiErrorType::Permission, "permission_error"),
        (ApiErrorType::NotFound, "not_found_error"),
        (ApiErrorType::Conflict, "conflict_error"),
        (ApiErrorType::RequestTooLarge, "request_too_large"),
        (ApiErrorType::RateLimit, "rate_limit_error"),
        (ApiErrorType::Api, "api_error"),
        (ApiErrorType::Timeout, "timeout_error"),
        (ApiErrorType::Overloaded, "overloaded_error"),
    ] {
        assert_eq!(value.to_string(), expected);
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            serde_json::to_string(expected).unwrap()
        );
    }
    assert_eq!(
        ApiErrorType::Other("future/code".into()).to_string(),
        "future/code"
    );
}
