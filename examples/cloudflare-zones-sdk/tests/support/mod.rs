//! Shared fixture helpers. Every response body comes from `fixtures/` (see `PROVENANCE.md`) or is
//! written inline in the test that needs a malformed one.

#![allow(dead_code)]

use concord_example_testserver::{Reply, TestServer};
use concord_sdk_cloudflare_zones::{Cloudflare, SendPolicy};

/// A synthetic API token; never a real credential.
pub const TOKEN: &str = "synthetic-cf-token-0123456789abcdef";
/// The synthetic account of the zone fixtures.
pub const ACCOUNT: &str = "acc0000000000000000000000000000a";
/// The zone on the second List Zones page, used for the DNS workflow.
pub const ZONE: &str = "1a00000000000000000000000000000f";
/// The record of the DNS Record Details fixture.
pub const RECORD: &str = "2b00000000000000000000000000000b";

/// The text of `fixtures/{name}`.
pub fn fixture(name: &str) -> String {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A 200 `application/json` reply with a fixture body.
pub fn ok(name: &str) -> Reply {
    Reply::json(200, fixture(name))
}

/// A reply with a status and a fixture body labelled JSON.
pub fn json(status: u16, name: &str) -> Reply {
    Reply::json(status, fixture(name))
}

/// The SDK's client pointed at the loopback server, with the default policy.
pub fn client(server: &TestServer) -> Cloudflare {
    Cloudflare::connect(concord_sdk_cloudflare_zones::http_client().unwrap(), TOKEN)
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
}

/// The client with a send policy.
pub fn client_with(server: &TestServer, policy: SendPolicy) -> Cloudflare {
    client(server).set_policy(policy)
}

/// A 404 that names the unexpected target, so a wrong request fails loudly.
pub fn unexpected(target: &str) -> Reply {
    Reply::text(404, "text/plain", format!("no fixture for {target}"))
}
