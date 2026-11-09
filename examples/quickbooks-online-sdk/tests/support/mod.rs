//! Shared test support: clients wired to the loopback server, fixtures, and a stateful fake of
//! the documented QuickBooks Online rules (synthetic: it implements the cited rules, it is not a
//! recording of the service).
//!
//! The fake's rules, each from PROVENANCE.md: tokens are issued per realm by the token endpoint
//! (Basic client credentials, form body); an access token is accepted only on its own realm's
//! paths (401 otherwise); a refresh may rotate the refresh token, and a replaced or unknown one
//! is `400 invalid_grant` (RFC 6749 §5.2); an authorization code works once; a create with a
//! `requestid` already seen in the realm returns the original response.

// Each test binary uses a different subset of these helpers.
#![allow(dead_code)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_quickbooks_online::concord::reqwest;
use concord_sdk_quickbooks_online::{
    Connection, Environment, OAuthApp, QuickBooks, RealmId, RefreshToken,
};
use serde_json::{Value, json};

pub const CLIENT_ID: &str = "synthetic-client-id";
pub const CLIENT_SECRET: &str = "synthetic-client-secret-9c1e";
/// `Basic base64("synthetic-client-id:synthetic-client-secret-9c1e")`, computed outside the SDK.
pub const BASIC: &str = "Basic c3ludGhldGljLWNsaWVudC1pZDpzeW50aGV0aWMtY2xpZW50LXNlY3JldC05YzFl";
pub const REDIRECT: &str = "https://app.example.test/oauth/callback";
pub const REALM_A: &str = "9130350000000001";
pub const REALM_B: &str = "9130350000000002";
pub const TOKEN_PATH: &str = "/oauth2/v1/tokens/bearer";

/// Reads `fixtures/<name>`.
pub fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Parses `fixtures/<name>` as JSON.
pub fn fixture_json(name: &str) -> Value {
    serde_json::from_slice(&fixture(name)).unwrap()
}

/// A JSON reply with a fixture body, labelled as the documented responses are.
pub fn json_fixture(status: u16, name: &str) -> Reply {
    Reply::text(status, "application/json;charset=UTF-8", fixture(name))
}

/// A transport like the SDK's `http_client()`: no retries, no redirects.
pub fn http(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout)
        .build()
        .unwrap()
}

pub fn realm(id: &str) -> RealmId {
    RealmId::new(id).unwrap()
}

/// The sandbox app, its token endpoint at the server.
pub fn app(server: &TestServer) -> OAuthApp {
    OAuthApp::new(Environment::Sandbox, CLIENT_ID, CLIENT_SECRET, REDIRECT)
        .unwrap()
        .with_token_base_url(&server.url())
        .unwrap()
}

/// A stored connection for `realm` with `refresh_token`.
pub fn connection(server: &TestServer, realm_id: &str, refresh_token: &str) -> Connection {
    app(server).connection(realm(realm_id), RefreshToken::new(refresh_token).unwrap())
}

/// `QuickBooks` for `connection`, pointed at the server.
pub fn qbo(server: &TestServer, connection: Connection) -> QuickBooks {
    qbo_with(server, connection, Duration::from_secs(10))
}

pub fn qbo_with(server: &TestServer, connection: Connection, timeout: Duration) -> QuickBooks {
    QuickBooks::connect(
        http(timeout),
        connection.for_fixture(&server.url()).unwrap(),
    )
}

/// The realm in an Accounting request path (`/v3/company/{realm}/..`).
pub fn path_realm(request: &Recorded) -> Option<String> {
    let rest = request.path().strip_prefix("/v3/company/")?;
    Some(rest.split('/').next()?.to_owned())
}

/// The bearer token a request carried.
pub fn bearer(request: &Recorded) -> Option<String> {
    request
        .header("authorization")?
        .strip_prefix("Bearer ")
        .map(str::to_owned)
}

pub fn token_requests(server: &TestServer) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.path() == TOKEN_PATH)
        .collect()
}

pub fn api_requests(server: &TestServer) -> Vec<Recorded> {
    server
        .requests()
        .into_iter()
        .filter(|r| r.path().starts_with("/v3/company/"))
        .collect()
}

fn form(request: &Recorded) -> HashMap<String, String> {
    request.form_pairs().into_iter().collect()
}

fn time() -> &'static str {
    "2026-11-07T10:00:00.000-08:00"
}

#[derive(Default)]
struct State {
    /// refresh token -> realm
    refresh: HashMap<String, String>,
    /// access token -> realm
    access: HashMap<String, String>,
    /// authorization code -> realm
    codes: HashMap<String, String>,
    /// Access tokens the service now treats as expired (401).
    expired: HashSet<String>,
    rotate: bool,
    issued: u64,
    customers: HashMap<String, Vec<Value>>,
    invoices: HashMap<String, BTreeMap<String, Value>>,
    request_ids: HashMap<(String, String), Value>,
    next_invoice: u64,
    creates_received: usize,
    delay_next_create: Option<Duration>,
    fail_next_create: Option<u16>,
    hold_token: Option<concord_example_testserver::Gate>,
}

/// The stateful fake service (synthetic; see the module documentation).
#[derive(Clone, Default)]
pub struct Fake(Arc<Mutex<State>>);

impl Fake {
    /// A fake that rotates the refresh token on every refresh.
    pub fn new() -> Self {
        let fake = Self::default();
        fake.0.lock().unwrap().rotate = true;
        fake.0.lock().unwrap().next_invoice = 238;
        fake
    }

    pub async fn start(&self) -> TestServer {
        let fake = self.clone();
        TestServer::start(move |r| fake.respond(r)).await
    }

    pub fn grant_refresh(&self, realm: &str, token: &str) {
        self.0
            .lock()
            .unwrap()
            .refresh
            .insert(token.to_owned(), realm.to_owned());
    }

    pub fn grant_code(&self, realm: &str, code: &str) {
        self.0
            .lock()
            .unwrap()
            .codes
            .insert(code.to_owned(), realm.to_owned());
    }

    /// Loads the customers of the query page fixtures into `realm`.
    pub fn add_customers(&self, realm: &str, fixtures: &[&str]) {
        let mut state = self.0.lock().unwrap();
        let list = state.customers.entry(realm.to_owned()).or_default();
        for name in fixtures {
            let page = fixture_json(name);
            list.extend(
                page["QueryResponse"]["Customer"]
                    .as_array()
                    .unwrap()
                    .clone(),
            );
        }
    }

    /// The service now refuses this access token (expired or revoked on its side).
    pub fn expire_access(&self, token: &str) {
        self.0.lock().unwrap().expired.insert(token.to_owned());
    }

    /// Whether a refresh token is still accepted.
    pub fn refresh_valid(&self, token: &str) -> bool {
        self.0.lock().unwrap().refresh.contains_key(token)
    }

    pub fn invoices(&self, realm: &str) -> usize {
        self.0
            .lock()
            .unwrap()
            .invoices
            .get(realm)
            .map_or(0, BTreeMap::len)
    }

    pub fn creates_received(&self) -> usize {
        self.0.lock().unwrap().creates_received
    }

    /// The next create is committed, but its response is held for `delay`.
    pub fn delay_next_create(&self, delay: Duration) {
        self.0.lock().unwrap().delay_next_create = Some(delay);
    }

    /// The next create is committed, then answered with this 5xx status.
    pub fn fail_next_create(&self, status: u16) {
        self.0.lock().unwrap().fail_next_create = Some(status);
    }

    /// Token responses wait for this gate.
    pub fn hold_tokens(&self, gate: concord_example_testserver::Gate) {
        self.0.lock().unwrap().hold_token = Some(gate);
    }

    pub fn respond(&self, r: &Recorded) -> Reply {
        if r.method == "POST" && r.path() == TOKEN_PATH {
            let reply = self.token(r);
            return match self.0.lock().unwrap().hold_token.clone() {
                Some(gate) => reply.after(gate),
                None => reply,
            };
        }
        if r.path().starts_with("/v3/company/") {
            return self.accounting(r);
        }
        Reply::new(404)
    }

    fn token(&self, r: &Recorded) -> Reply {
        if r.header("authorization") != Some(BASIC) {
            return json_fixture(401, "oauth-invalid-client.json");
        }
        if r.header("content-type") != Some("application/x-www-form-urlencoded") {
            return Reply::json(400, r#"{"error":"invalid_request"}"#);
        }
        let form = form(r);
        let mut state = self.0.lock().unwrap();
        let (realm, keep) = match form.get("grant_type").map(String::as_str) {
            Some("refresh_token") => {
                let token = form.get("refresh_token").cloned().unwrap_or_default();
                let realm = if state.rotate {
                    state.refresh.remove(&token)
                } else {
                    state.refresh.get(&token).cloned()
                };
                (realm, (!state.rotate).then_some(token))
            }
            Some("authorization_code") => {
                if form.get("redirect_uri").map(String::as_str) != Some(REDIRECT) {
                    return json_fixture(400, "oauth-invalid-grant.json");
                }
                let code = form.get("code").cloned().unwrap_or_default();
                (state.codes.remove(&code), None)
            }
            _ => return Reply::json(400, r#"{"error":"unsupported_grant_type"}"#),
        };
        let Some(realm) = realm else {
            return json_fixture(400, "oauth-invalid-grant.json");
        };
        state.issued += 1;
        let n = state.issued;
        let access = format!("synthetic-access-{realm}-{n}");
        let refresh = keep.unwrap_or_else(|| format!("synthetic-refresh-{realm}-{n}"));
        state.access.insert(access.clone(), realm.clone());
        state.refresh.insert(refresh.clone(), realm);
        Reply::json(
            200,
            json!({
                "token_type": "bearer",
                "expires_in": 3600,
                "refresh_token": refresh,
                "x_refresh_token_expires_in": 8640000,
                "access_token": access,
            })
            .to_string(),
        )
    }

    fn accounting(&self, r: &Recorded) -> Reply {
        let realm = path_realm(r).unwrap();
        let mut state = self.0.lock().unwrap();
        let authorized = bearer(r).is_some_and(|token| {
            !state.expired.contains(&token) && state.access.get(&token) == Some(&realm)
        });
        if !authorized {
            return json_fixture(401, "fault-authentication.json");
        }
        if r.query_all("minorversion") != ["75"] {
            return json_fixture(400, "fault-validation.json");
        }
        let rest = r.path()[format!("/v3/company/{realm}/").len()..].to_owned();
        match (
            r.method.as_str(),
            rest.split('/').collect::<Vec<_>>().as_slice(),
        ) {
            ("GET", ["query"]) => {
                let statement = r.query_all("query").pop().unwrap_or_default();
                let (start, max) = positions(&statement);
                let mut all = state.customers.get(&realm).cloned().unwrap_or_default();
                if let Some(name) = display_name_equals(&statement) {
                    all.retain(|c| {
                        c["DisplayName"]
                            .as_str()
                            .is_some_and(|d| d.eq_ignore_ascii_case(&name))
                    });
                }
                let page: Vec<Value> = all.into_iter().skip(start - 1).take(max).collect();
                let body = if page.is_empty() {
                    json!({"QueryResponse": {}, "time": time()})
                } else {
                    json!({"QueryResponse": {"Customer": page, "startPosition": start,
                        "maxResults": page.len()}, "time": time()})
                };
                Reply::json(200, body.to_string())
            }
            ("GET", ["customer", id]) => {
                let found = state
                    .customers
                    .get(&realm)
                    .and_then(|list| list.iter().find(|c| c["Id"] == *id).cloned());
                match found {
                    Some(customer) => Reply::json(
                        200,
                        json!({"Customer": customer, "time": time()}).to_string(),
                    ),
                    None => json_fixture(400, "fault-object-not-found.json"),
                }
            }
            ("POST", ["invoice"]) => {
                state.creates_received += 1;
                let request_id = r.query_all("requestid").pop().unwrap_or_default();
                let key = (realm.clone(), request_id);
                if let Some(original) = state.request_ids.get(&key) {
                    return Reply::json(200, original.to_string());
                }
                let request: Value = serde_json::from_slice(&r.body).unwrap();
                let id = state.next_invoice.to_string();
                state.next_invoice += 1;
                let invoice = invoice_from(&id, &request);
                let body = json!({"Invoice": invoice, "time": time()});
                state
                    .invoices
                    .entry(realm.clone())
                    .or_default()
                    .insert(id, invoice);
                state.request_ids.insert(key, body.clone());
                if let Some(status) = state.fail_next_create.take() {
                    return json_fixture(status, "fault-system.json");
                }
                let reply = Reply::json(200, body.to_string());
                match state.delay_next_create.take() {
                    Some(delay) => reply.delay(delay),
                    None => reply,
                }
            }
            ("GET", ["invoice", id]) => match state.invoices.get(&realm).and_then(|m| m.get(*id)) {
                Some(invoice) => {
                    Reply::json(200, json!({"Invoice": invoice, "time": time()}).to_string())
                }
                None => json_fixture(400, "fault-object-not-found.json"),
            },
            _ => Reply::new(404),
        }
    }
}

/// The value of a `DisplayName = '..'` filter (the only filter the fake applies), unescaped.
fn display_name_equals(statement: &str) -> Option<String> {
    let rest = statement.split_once("DisplayName = '")?.1;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => value.push(chars.next()?),
            '\'' => return Some(value),
            c => value.push(c),
        }
    }
    None
}

/// `STARTPOSITION n MAXRESULTS m` of a statement (defaults 1 and 100).
fn positions(statement: &str) -> (usize, usize) {
    let words: Vec<&str> = statement.split_whitespace().collect();
    let after = |key: &str, default: usize| {
        words
            .iter()
            .position(|w| w.eq_ignore_ascii_case(key))
            .and_then(|i| words.get(i + 1))
            .and_then(|w| w.parse().ok())
            .unwrap_or(default)
    };
    (after("STARTPOSITION", 1), after("MAXRESULTS", 100))
}

/// The invoice the fake stores for a create request: the request's lines numbered, a subtotal
/// line, `TotalAmt` and `Balance` the sum of the line amounts.
fn invoice_from(id: &str, request: &Value) -> Value {
    let mut lines = Vec::new();
    let mut total = 0.0;
    for (n, line) in request["Line"].as_array().unwrap().iter().enumerate() {
        let mut line = line.clone();
        total += line["Amount"].as_f64().unwrap();
        line["Id"] = json!((n + 1).to_string());
        line["LineNum"] = json!(n + 1);
        lines.push(line);
    }
    lines.push(
        json!({"DetailType": "SubTotalLineDetail", "Amount": total, "SubTotalLineDetail": {}}),
    );
    let mut invoice = json!({
        "domain": "QBO",
        "Id": id,
        "SyncToken": "0",
        "DocNumber": request.get("DocNumber").cloned().unwrap_or(json!(format!("10{id}"))),
        "TxnDate": "2026-11-07",
        "CustomerRef": request["CustomerRef"].clone(),
        "Line": lines,
        "TotalAmt": total,
        "Balance": total,
        "sparse": false,
    });
    invoice["MetaData"] = json!({"CreateTime": time(), "LastUpdatedTime": time()});
    invoice
}
