#![allow(dead_code)]
use concord_example_testserver::{Reply as WireReply, TestServer};
use concord_sdk_snowflake::*;
use std::num::NonZeroUsize;

pub const A: &str = "00000000-0000-4000-8000-000000000016";
pub const B: &str = "00000000-0000-4000-8000-000000000017";
pub const RID: &str = "00000000-0000-4000-8000-000000000101";
pub const CANCEL_ID: &str = "00000000-0000-4000-8000-000000000102";
pub const PENDING_A: &[u8] = include_bytes!("../../fixtures/pending-a.json");
pub const PENDING_B: &[u8] = include_bytes!("../../fixtures/pending-b.json");
pub const THREE: &[u8] = include_bytes!("../../fixtures/complete-3.json");
pub const TWO: &[u8] = include_bytes!("../../fixtures/complete-2.json");
pub const ONE: &[u8] = include_bytes!("../../fixtures/complete-1.json");
pub const COMPLETE_B: &[u8] = include_bytes!("../../fixtures/complete-b.json");
pub const CANCELLED: &[u8] = include_bytes!("../../fixtures/cancelled-b.json");
pub const NOT_FOUND: &[u8] = include_bytes!("../../fixtures/not-found-b.json");
pub const PART1: &[u8] = include_bytes!("../../fixtures/partition-1.json");
pub const PART2: &[u8] = include_bytes!("../../fixtures/partition-2.json");
pub const GZIP1: &[u8] = include_bytes!("../../fixtures/partition-1.json.gz");
pub const GZIP2: &[u8] = include_bytes!("../../fixtures/partition-2.json.gz");
pub fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}
pub fn id(s: &str) -> RequestId {
    RequestId::new(s).unwrap()
}
pub fn context() -> RequestContext {
    RequestContext {
        submission_id: id(RID),
    }
}
pub fn client(s: &TestServer) -> Client {
    Client::new(
        &s.url(),
        "SYNTHETIC_NOT_A_CREDENTIAL",
        TokenType::OAuth,
        ClientOptions::default(),
    )
    .unwrap()
}
pub fn statement() -> Statement {
    Statement {
        statement: "select \"value\" from DEMO where label = 'café'".into(),
        timeout: Some(60),
        session: SessionContext {
            database: Some("TEST_DB".into()),
            schema: Some("PUBLIC".into()),
            warehouse: Some("TEST_WH".into()),
            role: Some("TEST_ROLE".into()),
        },
    }
}
pub fn first(state: StatementState) -> FirstResult {
    match state {
        StatementState::Complete(f) => f,
        other => panic!("expected first result: {other:?}"),
    }
}
pub fn complete(outcome: PollOutcome) -> FirstResult {
    match outcome {
        PollOutcome::Complete(f) => *f,
        other => panic!("expected completion: {other:?}"),
    }
}
pub fn json(status: u16, body: &[u8]) -> WireReply {
    WireReply::json(status, body).header("x-snowflake-request-id", "synthetic-response-id")
}
pub fn gzip(body: &[u8]) -> WireReply {
    json(200, body).header("content-encoding", "gzip")
}
pub fn mutate(body: &[u8], edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut value = serde_json::from_slice(body).unwrap();
    edit(&mut value);
    serde_json::to_vec(&value).unwrap()
}
/// Keeps the paused runtime runnable while loopback I/O completes; no virtual auto-jump to an
/// HTTP deadline before the reactor observes the response. Tests advance Tokio time explicitly.
pub async fn spin_until(mut condition: impl FnMut() -> bool) {
    for _ in 0..200_000 {
        if condition() {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("loopback state did not progress");
}
