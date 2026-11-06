#![allow(dead_code)]

use concord::reqwest;
use concord_example_testserver::{Recorded, TestServer};
use concord_sdk_slack::{Client, ClientConfig};

pub fn client(server: &TestServer) -> Client {
    Client::new(
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        ClientConfig {
            token: "synthetic-slack-token".into(),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap()
}

pub fn assert_get(request: &Recorded, target: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(
        request.header_all("authorization"),
        ["Bearer synthetic-slack-token"]
    );
    assert_eq!(request.header_all("accept"), ["application/json"]);
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body, b"");
    assert_eq!(request.body_len, 0);
    assert!(!request.chunked);
    assert!(
        !request
            .query_all("token")
            .iter()
            .any(|value| !value.is_empty())
    );
}

pub fn fixture(name: &str) -> &'static [u8] {
    match name {
        "users-first" => include_bytes!("../../fixtures/users-first.json"),
        "users-empty-continuing" => include_bytes!("../../fixtures/users-empty-continuing.json"),
        "users-last-empty" => include_bytes!("../../fixtures/users-last-empty.json"),
        "users-last-null" => include_bytes!("../../fixtures/users-last-null.json"),
        "users-last-missing-cursor" => {
            include_bytes!("../../fixtures/users-last-missing-cursor.json")
        }
        "users-last-missing-metadata" => {
            include_bytes!("../../fixtures/users-last-missing-metadata.json")
        }
        "users-last-null-metadata" => {
            include_bytes!("../../fixtures/users-last-null-metadata.json")
        }
        "users-whitespace-cursor" => include_bytes!("../../fixtures/users-whitespace-cursor.json"),
        "conversation" => include_bytes!("../../fixtures/conversation.json"),
        "conversation-dm" => include_bytes!("../../fixtures/conversation-dm.json"),
        "channel-not-found" => include_bytes!("../../fixtures/channel-not-found.json"),
        "invalid-cursor" => include_bytes!("../../fixtures/invalid-cursor.json"),
        "missing-scope" => include_bytes!("../../fixtures/missing-scope.json"),
        "unknown-error" => include_bytes!("../../fixtures/unknown-error.json"),
        "rate-limited" => include_bytes!("../../fixtures/rate-limited.json"),
        "missing-members" => include_bytes!("../../fixtures/missing-members.json"),
        "missing-channel" => include_bytes!("../../fixtures/missing-channel.json"),
        "missing-ok" => include_bytes!("../../fixtures/missing-ok.json"),
        "missing-error" => include_bytes!("../../fixtures/missing-error.json"),
        "conflicting-error" => include_bytes!("../../fixtures/conflicting-error.json"),
        "invalid-cursor-type" => include_bytes!("../../fixtures/invalid-cursor-type.json"),
        "malformed" => include_bytes!("../../fixtures/malformed.json"),
        "proxy-error" => include_bytes!("../../fixtures/proxy-error.html"),
        _ => panic!("unknown fixture {name}"),
    }
}
