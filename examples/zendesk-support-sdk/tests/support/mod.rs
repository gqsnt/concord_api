//! Shared test support: a client wired to the loopback server, fixtures, and a stateful fake of
//! Zendesk's documented ticket-create idempotency (synthetic: it implements the documented rules,
//! it is not a recording of the service).

// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_zendesk_support::concord::reqwest;
use concord_sdk_zendesk_support::{SendPolicy, Zendesk};
use serde_json::{Value, json};

/// A synthetic OAuth access token; no test may find it in formatted output.
pub const TOKEN: &str = "synthetic-oauth-token-7f3a9c";

/// Reads `fixtures/<name>`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A JSON reply with a fixture body.
pub fn json_fixture(status: u16, name: &str) -> Reply {
    Reply::json(status, fixture(name))
}

/// A transport like the SDK's `http_client()`, resolving the account host to the server.
pub fn http(server: &TestServer, timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .resolve("acme.zendesk.test", server.addr())
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .unwrap()
}

/// `Zendesk` for subdomain `acme`, pointed at the server (`http://acme.zendesk.test:<port>`).
pub fn api(server: &TestServer) -> Zendesk {
    api_with(server, Duration::from_secs(10), SendPolicy::default())
}

/// [`api`] with a request timeout and a send policy.
pub fn api_with(server: &TestServer, timeout: Duration, policy: SendPolicy) -> Zendesk {
    Zendesk::connect(http(server, timeout), "acme", TOKEN)
        .unwrap()
        .set_policy(policy)
        .with_base_url(format!("http://zendesk.test:{}", server.addr().port()))
        .unwrap()
}

/// The arrival time of every request, for timing assertions.
#[derive(Clone, Default)]
pub struct Arrivals(Arc<Mutex<Vec<(String, Instant)>>>);

impl Arrivals {
    pub fn record(&self, r: &Recorded) {
        self.0
            .lock()
            .unwrap()
            .push((format!("{} {}", r.method, r.target), Instant::now()));
    }

    pub fn all(&self) -> Vec<(String, Instant)> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Default)]
struct State {
    tickets: BTreeMap<u64, Value>,
    keys: HashMap<String, (Value, u64)>,
    creates_received: usize,
    delay_next_create: Option<Duration>,
}

/// A fake of the documented ticket rules: Create Ticket assigns ids from 9001 and status `new`;
/// a repeated `Idempotency-Key` with the same body replays the original response with
/// `x-idempotency-lookup: hit`, with another body is `400 IdempotentRequestError`; Show Ticket
/// and List Tickets (`external_id`) read the stored tickets.
#[derive(Clone, Default)]
pub struct FakeZendesk(Arc<Mutex<State>>);

impl FakeZendesk {
    /// The next create is stored, then answered only after `delay` (a client that times out
    /// earlier never sees the response).
    pub fn delay_next_create(&self, delay: Duration) {
        self.0.lock().unwrap().delay_next_create = Some(delay);
    }

    /// Tickets that exist in the fake account.
    pub fn ticket_count(&self) -> usize {
        self.0.lock().unwrap().tickets.len()
    }

    /// Create requests that reached the fake (hits included).
    pub fn creates_received(&self) -> usize {
        self.0.lock().unwrap().creates_received
    }

    /// Answers one request.
    pub fn respond(&self, r: &Recorded) -> Reply {
        match (r.method.as_str(), r.path()) {
            ("POST", "/api/v2/tickets") => self.create(r),
            ("GET", "/api/v2/tickets") => self.list(r),
            ("GET", path) => match path
                .strip_prefix("/api/v2/tickets/")
                .and_then(|id| id.parse::<u64>().ok())
            {
                Some(id) => self.show(id),
                None => json_fixture(404, "error-not-found.json"),
            },
            _ => json_fixture(404, "error-not-found.json"),
        }
    }

    fn create(&self, r: &Recorded) -> Reply {
        let Ok(body) = serde_json::from_slice::<Value>(&r.body) else {
            return Reply::text(400, "text/plain", "Synthetic: unparseable JSON");
        };
        let key = r.header("idempotency-key").map(str::to_owned);
        let mut state = self.0.lock().unwrap();
        state.creates_received += 1;
        if let Some(key) = &key
            && let Some((original, id)) = state.keys.get(key)
        {
            if *original != body {
                return json_fixture(400, "error-idempotent-mismatch.json");
            }
            let ticket = state.tickets[id].clone();
            return Reply::json(201, json!({ "ticket": ticket }).to_string())
                .header("x-idempotency-lookup", "hit");
        }
        let id = 9001 + state.tickets.len() as u64;
        let new = &body["ticket"];
        let ticket = json!({
            "id": id,
            "url": format!("https://acme.zendesk.com/api/v2/tickets/{id}.json"),
            "external_id": new.get("external_id").cloned().unwrap_or(Value::Null),
            "type": new.get("type").cloned().unwrap_or(Value::Null),
            "subject": new.get("subject").cloned().unwrap_or(Value::Null),
            "description": new["comment"]["body"].clone(),
            "priority": new.get("priority").cloned().unwrap_or(Value::Null),
            "status": "new",
            "requester_id": 2001,
            "submitter_id": 2001,
            "assignee_id": null,
            "organization_id": null,
            "group_id": null,
            "tags": new.get("tags").cloned().unwrap_or(json!([])),
            "created_at": "2026-11-07T12:00:00Z",
            "updated_at": "2026-11-07T12:00:00Z"
        });
        state.tickets.insert(id, ticket.clone());
        let mut reply = Reply::json(201, json!({ "ticket": ticket }).to_string()).header(
            "location",
            format!("https://acme.zendesk.com/api/v2/tickets/{id}.json"),
        );
        if let Some(key) = key {
            state.keys.insert(key, (body, id));
            reply = reply.header("x-idempotency-lookup", "miss");
        }
        match state.delay_next_create.take() {
            Some(delay) => reply.delay(delay),
            None => reply,
        }
    }

    fn show(&self, id: u64) -> Reply {
        match self.0.lock().unwrap().tickets.get(&id) {
            Some(ticket) => Reply::json(200, json!({ "ticket": ticket }).to_string()),
            None => json_fixture(404, "error-not-found.json"),
        }
    }

    fn list(&self, r: &Recorded) -> Reply {
        let wanted = r.query_all("external_id");
        let state = self.0.lock().unwrap();
        let tickets: Vec<&Value> = state
            .tickets
            .values()
            .filter(|t| wanted.is_empty() || wanted.iter().any(|w| t["external_id"] == *w))
            .collect();
        let page = json!({
            "tickets": tickets,
            "meta": { "has_more": false, "after_cursor": null, "before_cursor": null },
            "links": { "next": null, "prev": null }
        });
        Reply::json(200, page.to_string())
    }
}
