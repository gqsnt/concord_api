#![cfg(feature = "json")]
//! An API that reports its errors inside a `200`, such as `{"ok": false, "error": "…"}`.
//! An arm that decodes the body itself fails with the decoder's error, and the call's error
//! names the location of the fault, as it does under a plain `read Json;`.

use concord::bytes::Bytes;
use concord::error::{Kept, Kind};
use concord::{fake, reply, Binary, Fail, Json};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::error::Error as _;

#[derive(Debug, Deserialize, PartialEq)]
struct Posted {
    channel: String,
    kind: MessageKind,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum MessageKind {
    Message,
}

#[derive(Debug, Deserialize)]
struct Refusal {
    error: String,
}

/// The arm of every 2xx: `ok` says whether the body is the value or the problem.
fn envelope<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<Refusal>> {
    #[derive(Deserialize)]
    struct Head {
        ok: bool,
    }
    let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    if head.ok {
        Json::decode(&body).map_err(Fail::from_read)
    } else {
        let problem: Refusal = Json::decode(&body).map_err(Fail::from_read)?;
        Err(problem)?
    }
}

concord::api! {
    client Plain {
        base "https://api.test";
        read Json;
        problem Refusal;
        GET post() -> Posted { path "post"; }
        GET taken() -> Result<Posted, Refusal> { path "taken"; status { 409 { body } => Err(body) } }
    }
}

concord::api! {
    client Wrapped {
        base "https://api.test";
        read Binary;
        problem Refusal read Json;
        status { 2xx { body } => envelope(body)? }
        GET post() -> Posted { path "post"; }
        GET taken() -> Result<Posted, Refusal> {
            path "taken";
            status {
                override 2xx { body } => Ok(envelope(body)?),
                409 { body } => Err(Json::decode(&body).map_err(Fail::from_read)?),
            }
        }
    }
}

fn sources(error: &dyn std::error::Error) -> Vec<String> {
    let mut found = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        found.push(cause.to_string());
        source = cause.source();
    }
    found
}

#[tokio::test]
async fn c5_8_an_arm_that_decodes_fails_with_the_error_of_read_json() {
    const BODY: &str = r#"{"ok": true, "channel": 5, "kind": "message"}"#;
    let plain = Plain::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let plain = plain.post().await.unwrap_err();
    let wrapped = wrapped.post().await.unwrap_err();

    assert_eq!(
        plain.to_string(),
        "mismatch: status 200, channel could not be decoded, in plain.post"
    );
    assert_eq!(
        wrapped.to_string(),
        "mismatch: status 200, channel could not be decoded, in wrapped.post"
    );
    assert_eq!(wrapped.kind(), Kind::Mismatch);
    assert_eq!(plain.reason(), None);
    assert_eq!(wrapped.reason(), None);
    assert_eq!(sources(&plain), ["channel could not be decoded"]);
    assert_eq!(sources(&wrapped), sources(&plain));
    assert_eq!(wrapped.body(), Some(BODY.as_bytes()));
    assert_eq!(wrapped.kept(), Some(Kept::Complete));
    let details = wrapped.details().to_string();
    assert!(
        details
            .contains("reader: invalid type: integer `5`, expected a string at line 1 column 25"),
        "{details}"
    );
    assert_eq!(
        details.replace("wrapped.post", "plain.post"),
        plain.details().to_string()
    );
}

#[tokio::test]
async fn c5_8_a_problem_inside_a_200_is_refused_with_the_problem() {
    let wrapped = Wrapped::new().with_transport(fake(|_| {
        Ok(reply(
            200,
            r#"{"ok": true, "channel": "C1", "kind": "message"}"#,
        ))
    }));
    assert_eq!(
        wrapped.post().await.unwrap(),
        Posted {
            channel: "C1".to_owned(),
            kind: MessageKind::Message
        }
    );
    let wrapped = Wrapped::new().with_transport(fake(|_| {
        Ok(reply(200, r#"{"ok": false, "error": "channel_not_found"}"#))
    }));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Refused);
    assert!(error.is_status(200));
    assert_eq!(
        error.problem().map(|problem| problem.error.as_str()),
        Some("channel_not_found")
    );
    assert_eq!(error.to_string(), "refused: status 200, in wrapped.post");
}

#[tokio::test]
async fn c5_8_an_envelope_without_ok_is_a_mismatch_with_the_decoder_message() {
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, r#"{"channel": "C1"}"#))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(error.kind(), Kind::Mismatch);
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, the body could not be decoded, in wrapped.post"
    );
    assert!(error
        .details()
        .to_string()
        .contains("reader: missing field `ok`"));
}

#[tokio::test]
async fn c5_3_an_arm_that_decodes_on_another_status_is_refused_with_the_bytes() {
    const BODY: &str = r#"{"error": 7}"#;
    let plain = Plain::new().with_transport(fake(|_| Ok(reply(409, BODY))));
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(409, BODY))));
    let plain = plain.taken().await.unwrap_err();
    let wrapped = wrapped.taken().await.unwrap_err();
    for (error, endpoint) in [(&plain, "plain.taken"), (&wrapped, "wrapped.taken")] {
        assert_eq!(error.kind(), Kind::Refused);
        assert!(error.is_status(409));
        assert!(error.problem().is_none());
        assert_eq!(error.body(), Some(BODY.as_bytes()));
        assert_eq!(error.kept(), Some(Kept::Complete));
        assert!(error.source().is_none());
        assert_eq!(
            error.to_string(),
            format!("refused: status 409, in {endpoint}")
        );
    }
    assert_eq!(
        wrapped
            .details()
            .to_string()
            .replace("wrapped.taken", "plain.taken"),
        plain.details().to_string()
    );
}

#[tokio::test]
async fn c5_8_the_message_and_the_causes_of_an_arm_that_decodes_hold_no_value() {
    const BODY: &str = r#"{"ok": true, "channel": "C1", "kind": "PRIVATE_SENTINEL"}"#;
    let wrapped = Wrapped::new().with_transport(fake(|_| Ok(reply(200, BODY))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, kind could not be decoded, in wrapped.post"
    );
    let mut cause: Option<&dyn std::error::Error> = Some(&error);
    while let Some(source) = cause {
        assert!(!format!("{source} {source:?} {source:#?}").contains("PRIVATE_SENTINEL"));
        cause = source.source();
    }
    assert!(error.details().to_string().contains("PRIVATE_SENTINEL"));
}

#[tokio::test]
async fn c5_6_an_arm_that_decodes_keeps_at_most_64_kib_of_the_bytes() {
    let pad = "x".repeat(70 * 1024);
    let body = format!(r#"{{"ok": true, "channel": 5, "kind": "message", "pad": "{pad}"}}"#);
    let wrapped = Wrapped::new().with_transport(fake(move |_| Ok(reply(200, body.clone()))));
    let error = wrapped.post().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "mismatch: status 200, channel could not be decoded, in wrapped.post"
    );
    assert_eq!(error.kept(), Some(Kept::Cut));
    assert_eq!(error.body().map(<[u8]>::len), Some(64 * 1024));
}
