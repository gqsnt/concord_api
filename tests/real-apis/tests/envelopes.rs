//! The Slack and Telegram clients of the example pages report an error inside a `200`. A
//! success that does not fit the type still names the field where it stopped fitting.
#![allow(clippy::unwrap_used)]

use concord::error::Kind;
use concord::{fake, reply, Secret};
use real_apis::slack::Slack;
use real_apis::telegram::Telegram;

#[tokio::test]
async fn c5_8_slack_names_the_field_of_a_success_that_does_not_fit() {
    let slack = Slack::new(Secret::new("xoxb-test")).with_transport(fake(|_| {
        Ok(reply(200, r#"{"ok": true, "team_id": 5, "user_id": "U1"}"#))
    }));
    let error = slack.auth_test().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, team_id could not be decoded, in slack.auth_test"
    );
    assert_eq!(error.reason(), None);
    assert!(error.details().to_string().contains("at line 1 column 25"));

    let slack = Slack::new(Secret::new("xoxb-test")).with_transport(fake(|_| {
        Ok(reply(200, r#"{"ok": false, "error": "invalid_auth"}"#))
    }));
    let error = slack.auth_test().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(
        error.problem().map(|problem| problem.error.as_str()),
        Some("invalid_auth")
    );
}

#[tokio::test]
async fn c5_8_telegram_names_the_field_of_a_result_that_does_not_fit() {
    let bot = Telegram::new(Secret::new("123:test")).with_transport(fake(|_| {
        Ok(reply(200, r#"{"ok": true, "result": {"first_name": 7}}"#))
    }));
    let error = bot.get_me().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, result.first_name could not be decoded, in telegram.get_me"
    );
    assert_eq!(error.reason(), None);

    let bot = Telegram::new(Secret::new("123:test")).with_transport(fake(|_| {
        Ok(reply(
            200,
            r#"{"ok": false, "error_code": 400, "description": "Bad Request"}"#,
        ))
    }));
    let error = bot.get_me().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert_eq!(error.problem().map(|problem| problem.error_code), Some(400));
}
