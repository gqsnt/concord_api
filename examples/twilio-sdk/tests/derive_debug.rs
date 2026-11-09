//! Positive diagnostic assertions supplement the existing end-to-end secrecy canary.
use concord_sdk_twilio::{Message, MessageStatus};

fn message() -> Message {
    Message {
        sid: "SM_public_diagnostic".into(),
        account_sid: "private-account-canary".into(),
        status: MessageStatus::Undelivered,
        body: Some("private-body-canary".into()),
        from: Some("private-from-canary".into()),
        to: Some("private-to-canary".into()),
        error_code: Some(30007),
        error_message: Some("private-explanation-canary".into()),
    }
}

#[test]
fn identity_outcome_and_error_code_remain_visible_in_both_debug_modes() {
    let message = message();
    for shown in [format!("{message:?}"), format!("{message:#?}")] {
        for public in ["SM_public_diagnostic", "Undelivered", "30007"] {
            assert!(
                shown.contains(public),
                "missing diagnostic {public}: {shown}"
            );
        }
        for private in [
            "private-account-canary",
            "private-body-canary",
            "private-from-canary",
            "private-to-canary",
            "private-explanation-canary",
        ] {
            assert!(!shown.contains(private), "leaked {private}: {shown}");
        }
    }
    // Formatting changes no field access or provider data.
    assert_eq!(message.body.as_deref(), Some("private-body-canary"));
}

#[test]
fn absent_sensitive_message_fields_stay_absent_in_the_model() {
    let mut message = message();
    message.body = None;
    message.from = None;
    message.to = None;
    message.error_message = None;
    assert!(message.body.is_none());
    assert!(!format!("{message:?}").contains("canary"));
    assert!(!format!("{message:#?}").contains("canary"));
}
