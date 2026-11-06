#![allow(dead_code)]
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_microsoft_graph::{AuthContext, GraphClient};

pub const INITIAL: &[u8] = include_bytes!("../../fixtures/users-initial.json");
pub const EMPTY: &[u8] = include_bytes!("../../fixtures/users-empty-intermediate.json");
pub const FINAL: &[u8] = include_bytes!("../../fixtures/users-final.json");
pub const INCREMENTAL: &[u8] = include_bytes!("../../fixtures/users-incremental.json");
pub const NO_CHANGES: &[u8] = include_bytes!("../../fixtures/users-no-changes.json");
pub const GROUPS: &[u8] = include_bytes!("../../fixtures/groups-first.json");
pub const GROUPS_FINAL: &[u8] = include_bytes!("../../fixtures/groups-final.json");
pub const MEMBERS: &[u8] = include_bytes!("../../fixtures/members-first.json");
pub const MEMBERS_FINAL: &[u8] = include_bytes!("../../fixtures/members-final.json");
pub const MEMBERS_TWO: &[u8] = include_bytes!("../../fixtures/members-second-group.json");

pub fn reply(request: &Recorded, fixture: &[u8]) -> Reply {
    let base = format!("http://{}/v1.0", request.header("host").unwrap());
    let body = std::str::from_utf8(fixture)
        .unwrap()
        .replace("{{BASE}}", &base);
    Reply::json(200, body).header("request-id", "synthetic-success-id")
}
pub fn client(server: &TestServer, tenant: &str) -> GraphClient {
    GraphClient::for_loopback(
        AuthContext::new(tenant, "synthetic-principal-permissions-v1").unwrap(),
        format!("synthetic-token-{tenant}"),
        &format!("{}/v1.0", server.url()),
    )
    .unwrap()
}
pub fn assert_get(request: &Recorded, target: &str, tenant: &str) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, target);
    assert_eq!(
        request.header("authorization"),
        Some(format!("Bearer synthetic-token-{tenant}").as_str())
    );
    assert_eq!(request.header_all("authorization").len(), 1);
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.header("content-type"), None);
    assert_eq!(request.body.as_slice(), b"");
    assert_eq!(request.body_len, 0);
}
