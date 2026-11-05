//! A fixture-backed fake of the Stripe v1 lists used by ST-A, for the loopback test server.
//!
//! Pages come from `fixtures/st_a/*.json` (see `fixtures/MANIFEST.json` for provenance). The
//! fake applies the documented list rules independently of the SDK:
//!
//! - the first page has no cursor; `starting_after=<id>` selects the page after that object;
//!   an unknown cursor is a `400 resource_missing`; `starting_after` together with
//!   `ending_before` is a `400 parameters_exclusive` (the reference: "These parameters are
//!   mutually exclusive");
//! - `expand[]=data.customer`, `expand[]=data.parent.subscription_details.subscription` and
//!   `expand[]=data.source` replace those ids in every listed object with the full object (the
//!   reference: "Expansions on list requests start with the `data` property");
//! - the `Stripe-Account` header selects the account's data (`acct_ALPHA`, `acct_BETA`); any
//!   other account is a `403`.
//!
//! Faults are switched on per test: a `429` with `Stripe-Rate-Limited-Reason: global-rate` and
//! `Stripe-Should-Retry: true` on the second invoice page (once), and a non-terminal line page
//! with no objects.

use std::sync::{Arc, Mutex};

use concord_example_testserver::{Recorded, Reply};
use serde_json::Value;

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).expect("fixture is JSON")
}

struct Fixtures {
    invoices_page1: Value,
    invoices_page2: Value,
    lines_in_a3: Value,
    beta_invoices: Value,
    balance_page1: Value,
    balance_page2: Value,
    empty_terminal: Value,
    malformed_nonterminal: Value,
    rate_limited: Value,
    customers: Value,
    subscriptions: Value,
    sources: Value,
}

impl Fixtures {
    fn load() -> Self {
        Self {
            invoices_page1: fixture(include_str!("../fixtures/st_a/invoices_page1.json")),
            invoices_page2: fixture(include_str!("../fixtures/st_a/invoices_page2.json")),
            lines_in_a3: fixture(include_str!("../fixtures/st_a/invoice_lines_in_A3.json")),
            beta_invoices: fixture(include_str!("../fixtures/st_a/beta_invoices.json")),
            balance_page1: fixture(include_str!("../fixtures/st_a/balance_page1.json")),
            balance_page2: fixture(include_str!("../fixtures/st_a/balance_page2.json")),
            empty_terminal: fixture(include_str!("../fixtures/st_a/empty_terminal.json")),
            malformed_nonterminal: fixture(include_str!(
                "../fixtures/st_a/malformed_nonterminal.json"
            )),
            rate_limited: fixture(include_str!("../fixtures/st_a/rate_limited.json")),
            customers: fixture(include_str!("../fixtures/st_a/customers.json")),
            subscriptions: fixture(include_str!("../fixtures/st_a/subscriptions.json")),
            sources: fixture(include_str!("../fixtures/st_a/sources.json")),
        }
    }
}

/// Faults to inject.
#[derive(Clone, Copy, Debug, Default)]
pub struct Faults {
    /// Answer the second invoice page (`starting_after=in_A2`) with a rate-limited `429`, once.
    pub rate_limit_second_invoice_page: bool,
    /// Answer `in_A3`'s line continuation with `has_more: true` and no objects.
    pub malformed_lines: bool,
}

#[derive(Clone)]
pub struct BillingFake {
    fixtures: Arc<Fixtures>,
    faults: Arc<Mutex<Faults>>,
    counter: Arc<Mutex<u32>>,
    key: String,
}

impl BillingFake {
    pub fn new(secret_key: &str) -> Self {
        Self {
            fixtures: Arc::new(Fixtures::load()),
            faults: Arc::default(),
            counter: Arc::default(),
            key: secret_key.to_owned(),
        }
    }

    pub fn set_faults(&self, faults: Faults) {
        *self.faults.lock().unwrap() = faults;
    }

    pub fn respond(&self, request: &Recorded) -> Reply {
        let request_id = {
            let mut counter = self.counter.lock().unwrap();
            *counter += 1;
            format!("req_st_a_{:03}", *counter)
        };
        let reply = self.route(request);
        reply.header("request-id", request_id)
    }

    fn route(&self, request: &Recorded) -> Reply {
        let f = &self.fixtures;
        if request.header("authorization") != Some(&*format!("Bearer {}", self.key))
            || request.method != "GET"
        {
            return error(401, "invalid_request_error", "authentication");
        }
        if request.header("stripe-version") != Some("2026-10-28.endive") {
            return error(400, "invalid_request_error", "version_mismatch");
        }
        let after = request.query_all("starting_after");
        let before = request.query_all("ending_before");
        if !after.is_empty() && !before.is_empty() {
            return error(400, "invalid_request_error", "parameters_exclusive");
        }
        let after = after.first().map(String::as_str);
        let expand = request.query_all("expand[]");
        let account = request.header("stripe-account");
        let page = match (account, request.path(), after) {
            (Some("acct_ALPHA"), "/v1/invoices", None) => f.invoices_page1.clone(),
            (Some("acct_ALPHA"), "/v1/invoices", Some("in_A2")) => {
                let mut faults = self.faults.lock().unwrap();
                if faults.rate_limit_second_invoice_page {
                    faults.rate_limit_second_invoice_page = false;
                    return Reply::json(429, f.rate_limited.to_string())
                        .header("stripe-rate-limited-reason", "global-rate")
                        .header("stripe-should-retry", "true");
                }
                f.invoices_page2.clone()
            }
            (Some("acct_ALPHA"), "/v1/invoices/in_A3/lines", Some("il_A3b")) => {
                if self.faults.lock().unwrap().malformed_lines {
                    f.malformed_nonterminal.clone()
                } else {
                    f.lines_in_a3.clone()
                }
            }
            (Some("acct_ALPHA"), "/v1/balance_transactions", None) => f.balance_page1.clone(),
            (Some("acct_ALPHA"), "/v1/balance_transactions", Some("txn_A3")) => {
                f.balance_page2.clone()
            }
            (Some("acct_BETA"), "/v1/invoices", None) => f.beta_invoices.clone(),
            (Some("acct_BETA"), "/v1/balance_transactions", None) => f.empty_terminal.clone(),
            (Some("acct_ALPHA" | "acct_BETA"), _, _) => {
                return error(400, "invalid_request_error", "resource_missing");
            }
            _ => return error(403, "invalid_request_error", "account_invalid"),
        };
        Reply::json(200, self.expand(page, &expand).to_string())
    }

    fn expand(&self, mut page: Value, expand: &[String]) -> Value {
        let f = &self.fixtures;
        let replace = |slot: Option<&mut Value>, table: &Value| {
            if let Some(slot) = slot
                && let Some(id) = slot.as_str()
                && let Some(object) = table.get(id)
            {
                *slot = object.clone();
            }
        };
        let Some(data) = page.get_mut("data").and_then(Value::as_array_mut) else {
            return page;
        };
        for object in data {
            for path in expand {
                match path.as_str() {
                    "data.customer" => replace(object.get_mut("customer"), &f.customers),
                    "data.parent.subscription_details.subscription" => replace(
                        object.pointer_mut("/parent/subscription_details/subscription"),
                        &f.subscriptions,
                    ),
                    "data.source" => replace(object.get_mut("source"), &f.sources),
                    _ => {}
                }
            }
        }
        page
    }
}

fn error(status: u16, kind: &str, code: &str) -> Reply {
    Reply::json(
        status,
        format!(r#"{{"error":{{"type":"{kind}","code":"{code}","message":"synthetic {code}"}}}}"#),
    )
}
