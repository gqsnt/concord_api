#![allow(dead_code)] // This fixture vocabulary is shared by two independent test binaries.

use std::time::Duration;

use concord::reqwest;
use concord_example_testserver::{Reply, Script, TestServer};
use concord_sdk_twilio::{BasicAuth, CreateMessage, Twilio};

pub const ACCOUNT: &str = "AC_fixture_account";
pub const SID: &str = "SM_fixture_message";
pub const CREATE_PATH: &str = "/2010-04-01/Accounts/AC_fixture_account/Messages.json";
pub const FETCH_PATH: &str =
    "/2010-04-01/Accounts/AC_fixture_account/Messages/SM_fixture_message.json";
pub const AUTHORIZATION: &str = "Basic Zml4dHVyZS11c2VyOmZpeHR1cmUtcGFzc3dvcmQ=";
pub const FORM: &[u8] = include_bytes!("../../fixtures/create.form");
pub const INTENT_FORM: &[u8] = include_bytes!("../../fixtures/create-intent.form");
pub const CREATED: &[u8] = include_bytes!("../../fixtures/created.json");
pub const UNDELIVERED: &[u8] = include_bytes!("../../fixtures/undelivered.json");
pub const DELIVERED: &[u8] = include_bytes!("../../fixtures/delivered.json");
pub const UNKNOWN_STATUS: &[u8] = include_bytes!("../../fixtures/unknown-status.json");
pub const BAD_REQUEST: &[u8] = include_bytes!("../../fixtures/bad-request.json");
pub const UNAUTHORIZED: &[u8] = include_bytes!("../../fixtures/unauthorized.json");
pub const NOT_FOUND: &[u8] = include_bytes!("../../fixtures/not-found.json");
pub const RATE_LIMITED: &[u8] = include_bytes!("../../fixtures/rate-limited.json");
pub const MALFORMED: &[u8] = include_bytes!("../../fixtures/malformed.json");
pub const MISSING_STATUS: &[u8] = include_bytes!("../../fixtures/missing-status.json");
pub const WRONG_ERROR_CODE: &[u8] = include_bytes!("../../fixtures/wrong-error-code.json");
pub const NOT_AN_ERROR: &[u8] = include_bytes!("../../fixtures/not-an-error.json");
pub const HTML: &[u8] = include_bytes!("../../fixtures/html-error.html");

pub fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

pub fn client(base: &str) -> Twilio {
    Twilio::new(
        http(),
        ACCOUNT,
        BasicAuth::new("fixture-user", "fixture-password").unwrap(),
    )
    .with_base_url(base)
    .unwrap()
}

pub fn message() -> CreateMessage {
    CreateMessage::new("+15005550006", "+15005550001", "Hello café & + 🌍")
}

pub async fn scripted(replies: impl IntoIterator<Item = Reply>) -> (TestServer, Script, Twilio) {
    let script = Script::new(replies);
    let responder = script.clone();
    let server = TestServer::start(move |_| responder.next()).await;
    let client = client(&server.url());
    (server, script, client)
}
