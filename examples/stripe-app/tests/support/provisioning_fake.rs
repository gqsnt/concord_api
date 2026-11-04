//! A stateful fake of the Stripe v1 writes used by ST-B, for the loopback test server.
//!
//! It implements the documented idempotency behaviour (Stripe API reference, "Idempotent
//! requests" and "Advanced error handling", retrieved 2026-11-04) independently of the SDK:
//!
//! - the first `POST` with a given `Idempotency-Key` executes and its status and body are saved;
//! - a later `POST` with the same key and the **same endpoint and parameters** returns the saved
//!   response with `Idempotent-Replayed: true` and executes nothing;
//! - the same key with a **different endpoint or parameters** is refused with
//!   `type: idempotency_error` (HTTP 400 here: the reference documents the error type, not the
//!   status for this case; see `PROVIDER.md`);
//! - keys are scoped by the `Stripe-Account` header value (an inference from "unique ... on your
//!   account"; the tests do not depend on cross-account reuse).
//!
//! Parameters are compared as the decoded form pairs in order, with the test server's own form
//! decoder (not the SDK's encoder). A "lose once" switch executes and saves a write, then closes
//! the connection without answering: a response lost after the server accepted the write.
//!
//! Response objects are synthetic and contain the fields of the reference objects the SDK models
//! decode (provenance: `schema_derived_synthetic`, generated here; see `fixtures/MANIFEST.json`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use concord_example_testserver::{Recorded, Reply};

/// One executed (not replayed) write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Executed {
    pub account: Option<String>,
    pub path: String,
    pub object: String,
}

#[derive(Default)]
struct State {
    next_id: u32,
    next_request: u32,
    saved: HashMap<(Option<String>, String), Saved>,
    lose_once: HashSet<String>,
    executed: Vec<Executed>,
}

struct Saved {
    path: String,
    params: Vec<(String, String)>,
    status: u16,
    body: String,
}

/// The fake. Clone it into the responder closure; inspect it from the test.
#[derive(Clone, Default)]
pub struct FakeStripe {
    state: Arc<Mutex<State>>,
    key: String,
}

impl FakeStripe {
    pub fn new(secret_key: &str) -> Self {
        Self {
            state: Arc::default(),
            key: secret_key.to_owned(),
        }
    }

    /// The next write to `path` executes, is saved, and its response is lost.
    pub fn lose_response_once(&self, path: &str) {
        self.state.lock().unwrap().lose_once.insert(path.to_owned());
    }

    /// Writes that executed (replays excluded), in order.
    pub fn executed(&self) -> Vec<Executed> {
        self.state.lock().unwrap().executed.clone()
    }

    pub fn respond(&self, request: &Recorded) -> Reply {
        let mut state = self.state.lock().unwrap();
        state.next_request += 1;
        let request_id = format!("req_fake{:04}", state.next_request);
        if request.header("authorization") != Some(&*format!("Bearer {}", self.key)) {
            return error(
                401,
                "invalid_request_error",
                None,
                "Invalid API Key provided",
            )
            .header("request-id", request_id);
        }
        if request.header("stripe-version").is_none() || request.method != "POST" {
            return error(400, "invalid_request_error", None, "unexpected request")
                .header("request-id", request_id);
        }
        let account = request.header("stripe-account").map(str::to_owned);
        let key = request.header("idempotency-key").map(str::to_owned);
        let path = request.path().to_owned();
        let params = request.form_pairs();

        if let Some(key) = &key
            && let Some(saved) = state.saved.get(&(account.clone(), key.clone()))
        {
            if saved.path == path && saved.params == params {
                return Reply::json(saved.status, saved.body.clone())
                    .header("request-id", request_id)
                    .header("idempotent-replayed", "true");
            }
            return error(
                400,
                "idempotency_error",
                None,
                &format!(
                    "Keys for idempotent requests can only be used with the same parameters \
                     they were first used with. Try using a key other than '{key}' if you meant \
                     to execute a different request."
                ),
            )
            .header("request-id", request_id);
        }

        state.next_id += 1;
        let n = state.next_id;
        let Some((object, body)) = execute(&path, &params, n) else {
            return error(
                404,
                "invalid_request_error",
                Some("resource_missing"),
                "no route",
            )
            .header("request-id", request_id);
        };
        state.executed.push(Executed {
            account: account.clone(),
            path: path.clone(),
            object,
        });
        if let Some(key) = key {
            state.saved.insert(
                (account, key),
                Saved {
                    path: path.clone(),
                    params,
                    status: 200,
                    body: body.clone(),
                },
            );
        }
        if state.lose_once.remove(&path) {
            return Reply::hang_up();
        }
        Reply::json(200, body).header("request-id", request_id)
    }
}

fn error(status: u16, kind: &str, code: Option<&str>, message: &str) -> Reply {
    let code = code.map_or("null".to_owned(), |c| format!("\"{c}\""));
    let message = message.replace('"', "'");
    Reply::json(
        status,
        format!(r#"{{"error":{{"type":"{kind}","code":{code},"message":"{message}"}}}}"#),
    )
}

fn get<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn json_string(value: Option<&str>) -> String {
    value.map_or("null".to_owned(), |v| {
        serde_json::Value::String(v.to_owned()).to_string()
    })
}

fn metadata(params: &[(String, String)]) -> String {
    let entries: Vec<String> = params
        .iter()
        .filter_map(|(k, v)| {
            let name = k.strip_prefix("metadata[")?.strip_suffix(']')?;
            (!v.is_empty()).then(|| format!("{}:{}", json_string(Some(name)), json_string(Some(v))))
        })
        .collect();
    format!("{{{}}}", entries.join(","))
}

const CREATED: i64 = 1_767_225_600;

fn price_json(
    id: &str,
    product: &str,
    currency: &str,
    amount: &str,
    interval: Option<&str>,
) -> String {
    let recurring = interval.map_or("null".to_owned(), |i| {
        format!(r#"{{"interval":"{i}","interval_count":1,"trial_period_days":null,"usage_type":"licensed"}}"#)
    });
    let kind = if interval.is_some() {
        "recurring"
    } else {
        "one_time"
    };
    format!(
        r#"{{"id":"{id}","object":"price","active":true,"currency":"{currency}","product":"{product}",
        "unit_amount":{amount},"recurring":{recurring},"type":"{kind}","lookup_key":null,
        "nickname":null,"metadata":{{}},"created":{CREATED},"livemode":false}}"#
    )
}

/// Executes one write; `None` for an unknown route.
fn execute(path: &str, params: &[(String, String)], n: u32) -> Option<(String, String)> {
    match path {
        "/v1/products" => {
            let id = format!("prod_F{n:04}");
            let features: Vec<String> = params
                .iter()
                .filter(|(k, _)| k.starts_with("marketing_features[") && k.ends_with("[name]"))
                .map(|(_, v)| format!(r#"{{"name":{}}}"#, json_string(Some(v))))
                .collect();
            let body = format!(
                r#"{{"id":"{id}","object":"product","active":true,"name":{},"description":{},
                "default_price":null,"images":[],"marketing_features":[{}],"metadata":{},
                "created":{CREATED},"updated":{CREATED},"livemode":false}}"#,
                json_string(get(params, "name")),
                json_string(get(params, "description")),
                features.join(","),
                metadata(params)
            );
            Some((id, body))
        }
        "/v1/prices" => {
            let id = format!("price_F{n:04}");
            let body = price_json(
                &id,
                get(params, "product")?,
                get(params, "currency")?,
                get(params, "unit_amount")?,
                get(params, "recurring[interval]"),
            );
            Some((id, body))
        }
        "/v1/customers" => {
            let id = format!("cus_F{n:04}");
            Some((id.clone(), customer_json(&id, params)))
        }
        _ if path.starts_with("/v1/customers/") => {
            let id = path.trim_start_matches("/v1/customers/").to_owned();
            Some((id.clone(), customer_json(&id, params)))
        }
        "/v1/subscriptions" => {
            let id = format!("sub_F{n:04}");
            let customer = get(params, "customer")?;
            let price = get(params, "items[0][price]")?;
            let quantity = get(params, "items[0][quantity]").unwrap_or("1");
            let invoice_id = format!("in_F{n:04}");
            let expand: Vec<&str> = params
                .iter()
                .filter(|(k, _)| k == "expand[]")
                .map(|(_, v)| v.as_str())
                .collect();
            let latest_invoice = if expand.contains(&"latest_invoice") {
                format!(
                    r#"{{"id":"{invoice_id}","object":"invoice","customer":"{customer}","currency":"eur",
                    "status":"open","total":2400,"subtotal":2400,"amount_due":2400,"amount_paid":0,
                    "amount_remaining":2400,"collection_method":"charge_automatically",
                    "created":{CREATED},"period_start":{CREATED},"period_end":{CREATED},
                    "parent":{{"type":"subscription_details","subscription_details":{{"subscription":"{id}","metadata":{{}}}}}},
                    "lines":{{"object":"list","data":[],"has_more":false,"total_count":0,"url":"/v1/invoices/{invoice_id}/lines"}},
                    "metadata":{{}},"livemode":false}}"#
                )
            } else {
                format!("\"{invoice_id}\"")
            };
            let body = format!(
                r#"{{"id":"{id}","object":"subscription","customer":"{customer}","status":"active",
                "items":{{"object":"list","data":[{{"id":"si_F{n:04}","object":"subscription_item",
                "price":{},"quantity":{quantity},"current_period_start":{CREATED},
                "current_period_end":{CREATED},"subscription":"{id}","metadata":{{}}}}],
                "has_more":false,"total_count":1,"url":"/v1/subscription_items?subscription={id}"}},
                "latest_invoice":{latest_invoice},"currency":"eur","collection_method":"charge_automatically",
                "cancel_at_period_end":false,"description":null,"created":{CREATED},
                "start_date":{CREATED},"metadata":{},"livemode":false}}"#,
                price_json(price, "prod_unknown", "eur", "1200", Some("month")),
                metadata(params)
            );
            Some((id, body))
        }
        _ => None,
    }
}

fn customer_json(id: &str, params: &[(String, String)]) -> String {
    let locales: Vec<String> = params
        .iter()
        .filter(|(k, _)| k == "preferred_locales[]")
        .map(|(_, v)| json_string(Some(v)))
        .collect();
    format!(
        r#"{{"id":"{id}","object":"customer","email":{},"name":{},"description":null,
        "phone":null,"address":null,"balance":0,"currency":null,"created":{CREATED},
        "preferred_locales":[{}],"metadata":{},"livemode":false}}"#,
        json_string(get(params, "email")),
        json_string(get(params, "name")),
        locales.join(","),
        metadata(params)
    )
}
