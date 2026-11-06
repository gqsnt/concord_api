use concord::reqwest;
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_dropbox::{Client, ClientConfig, PathRoot, RequestContext};
use std::time::Duration;

pub const TOKEN: &str = "SYNTHETIC_DROPBOX_TOKEN_NOT_A_CREDENTIAL";
pub const MEMBER_A: &str = "dbmid:SYNTHETIC_MEMBER_A";
pub const MEMBER_B: &str = "dbmid:SYNTHETIC_MEMBER_B";
pub const A_FIRST: &[u8] = include_bytes!("../../fixtures/list-a-first.json");
pub const A_LAST: &[u8] = include_bytes!("../../fixtures/list-a-last.json");
pub const B_FIRST: &[u8] = include_bytes!("../../fixtures/list-b-first.json");
pub const B_LAST: &[u8] = include_bytes!("../../fixtures/list-b-last.json");

pub fn client(server: &TestServer) -> Client {
    Client::new(
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap(),
        ClientConfig {
            token: TOKEN.into(),
        },
    )
    .with_base_url(format!("{}/2", server.url()))
    .unwrap()
}

pub fn context_a() -> RequestContext {
    RequestContext::new(MEMBER_A, PathRoot::root("100").unwrap()).unwrap()
}

pub fn context_b() -> RequestContext {
    RequestContext::new(MEMBER_B, PathRoot::namespace("200").unwrap()).unwrap()
}

pub fn json(status: u16, bytes: &[u8], id: &str) -> Reply {
    Reply::json(status, bytes).header("X-Dropbox-Request-Id", id)
}

pub fn assert_request(request: &Recorded, member: &str, root: &str, target: &str, body: &[u8]) {
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, target);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {TOKEN}")]
    );
    assert_eq!(request.header_all("dropbox-api-select-user"), [member]);
    assert_eq!(request.header_all("dropbox-api-path-root"), [root]);
    assert!(request.header("dropbox-api-select-admin").is_none());
    assert_eq!(request.header("content-type"), Some("application/json"));
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.body, body);
    assert_eq!(request.body_len, body.len() as u64);
}
