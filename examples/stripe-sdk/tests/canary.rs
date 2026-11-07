//! Secrets: the secret key (`SecretKey`, `src/types.rs:230`; `StripeContext::new` builds
//! the `#[sensitive]` `Authorization: Bearer`, `src/lib.rs:172`), the context's connected
//! account (`with_account`; `#[sensitive] header("stripe-account")`, `src/lib.rs:178`) and a
//! per-request account (`AccountOverride::Account`, `#[sensitive] account_override`), and an
//! idempotency key (`IdempotencyKey`, `src/types.rs:270`; `#[sensitive] idempotency_key`).
//! Each has its own canary (with the prefix Stripe's validation requires). The keys travel as
//! headers. The server echoes the secret key into headers it does not project
//! (`x-request-id`, `set-cookie`; `Request-Id` is shown by design), the error `message` and
//! `param` (withheld by `ApiError`) and every malformed, wrong-media and truncated body. Read
//! (`customers().retrieve`, on the per-request account) and write (`customers().create`, with
//! the idempotency key) cover the seven scenarios through the SDK `send()`, the generated
//! `send()` and the native `Prepared`; a `list().pages()` traversal on the context's account
//! runs beside them. The generated `StripeConfig` is also formatted with an unflagged value.
//! Not covered: native reqwest values, `expose()` accessors, and canaries in decoded models
//! (`Customer` is returned in clear; the success bodies carry none). No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use std::{net::TcpListener, time::Duration};

fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder()
        .no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

fn refused() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    url
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(404, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
        .header("request-id", "req_synthetic")
        .header("x-request-id", echo)
        .header("set-cookie", echo)
}

async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> {
    let result = prepared.receive().await;
    c.assert_debug_absent("Received Result", &result);
    match result {
        Ok(received) => {
            c.assert_debug_absent("response head", &received.head());
            c.assert_debug_absent("response metadata", &received.meta());
            received.decode().await
        }
        Err(error) => Err(error),
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

use concord::reqwest::header::HeaderValue;
use concord_example_stripe_sdk::params::CustomerCreate;
use concord_example_stripe_sdk::{
    AccountId, AccountOverride, CustomerId, Error, IdempotencyKey, OnAccount, Response, SecretKey,
    Stripe, StripeConfig, StripeContext, send,
};

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.meta());
    if let Some(failure) = error.api() {
        c.assert_debug_absent("API failure", failure);
        c.assert_debug_absent("API error", &failure.error());
    }
    if let Some(e) = error.concord() {
        concord_error(c, e);
    }
}

fn sent<T: std::fmt::Debug>(c: &mut Canaries, label: &str, result: &Result<Response<T>, Error>) {
    c.assert_debug_absent(label, result);
    match result {
        Ok(response) => c.assert_debug_absent("response metadata", &response.meta),
        Err(e) => error(c, e),
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("stripe");
    let key = SecretKey::new(format!("sk_test_{}", c.issue())).unwrap();
    let account = AccountId::new(format!("acct_{}", c.issue())).unwrap();
    let per_request =
        AccountOverride::Account(AccountId::new(format!("acct_{}", c.issue())).unwrap());
    let idempotency = IdempotencyKey::new(c.issue()).unwrap();
    c.assert_debug_absent("secret key", &key);
    c.assert_debug_absent("account", &account);
    c.assert_debug_absent("account override", &per_request);
    c.assert_debug_absent("idempotency key", &idempotency);
    // The generated config, with a value the caller did not flag sensitive.
    let config = StripeConfig {
        authorization: HeaderValue::from_str(&format!("Bearer {}", key.expose())).unwrap(),
    };
    c.assert_debug_absent("generated config (value not flagged)", &config);
    c.assert_debug_absent("generated client from config", &Stripe::new(http(), config));
    let customer = CustomerId::new("cus_one").unwrap();
    for case in 0..7 {
        let echo = key.expose().to_owned();
        let server = TestServer::start(move |r: &Recorded| {
            let one = r#"{"id":"cus_one","object":"customer","email":"one@example.test","name":"One","description":null,"balance":0,"created":1767225600,"currency":"usd","metadata":{},"livemode":false}"#;
            let success = if r.method == "GET" && r.path() == "/v1/customers" {
                format!(r#"{{"object":"list","url":"/v1/customers","has_more":false,"data":[{one}]}}"#)
            } else {
                one.to_owned()
            };
            let problem = format!(
                r#"{{"error":{{"type":"invalid_request_error","code":"resource_missing","message":"{echo}","param":"{echo}","doc_url":"https://stripe.com/docs/error-codes/resource-missing"}}}}"#
            );
            reply(case, &success, &problem, &echo)
        })
        .await;
        let base = format!("{}/v1", if case == 6 { refused() } else { server.url() });
        let stripe = StripeContext::new(http(), &key)
            .with_base_url(&base)
            .unwrap()
            .with_account(Some(account.clone()));
        let customers = stripe.api().customers();
        let read = customers
            .retrieve(customer.clone())
            .on_account(&per_request);
        let write = customers
            .create(CustomerCreate {
                name: Some("One".into()),
                ..CustomerCreate::default()
            })
            .set_idempotency_key(idempotency.clone());
        c.assert_debug_absent("context", &stripe);
        c.assert_debug_absent("generated client", stripe.api());
        c.assert_debug_absent("customers scope", &customers);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
        let prepared = write.clone().prepare().unwrap();
        c.assert_debug_absent("write Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let generated = read.clone().send().await;
        c.assert_debug_absent("generated Result", &generated);
        assert_eq!(generated.is_ok(), case == 0);
        if let Err(e) = &generated {
            concord_error(&mut c, e);
        }
        let result = send(read.prepare().unwrap()).await;
        sent(&mut c, "read Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let result = send(write.prepare().unwrap()).await;
        sent(&mut c, "write Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        let mut pages = customers.list().pages().unwrap();
        c.assert_debug_absent("traversal", &pages);
        match pages.next_page().await {
            Ok(page) => {
                assert_eq!(case, 0);
                let page = page.unwrap();
                c.assert_debug_absent("page metadata", &page.meta);
                c.assert_debug_absent("cursor", &page.next);
            }
            Err(e) => {
                c.assert_debug_absent("PagesError", &e);
                c.assert_display_absent("PagesError", &e);
                c.assert_error_absent("PagesError and sources", &e);
                error(&mut c, e.provider_error().unwrap());
            }
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "stripe: {} renderings; positive control passed",
        c.checked()
    );
}
