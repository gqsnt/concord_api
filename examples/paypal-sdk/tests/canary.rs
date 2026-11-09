//! Secrets: client ID and client secret -> OAuth Basic header; returned access token ->
//! API bearer header. Distinct canaries in all three, with no wire exemptions.
//! Order read and capture write cover all seven cases after successful OAuth acquisition;
//! OAuth itself also covers all seven cases. Echoed message/description/value and identity
//! error_description exercise withheld provider fields. Native Prepared sends intentionally
//! bypass OAuth, as documented, and are checked for safe Concord diagnostics.
//! Excludes native reqwest, *_unredacted, clear order/capture/link fields and public error codes.
//! Session::new is crate-private; no public getter exposes Session/PayPalConfig for standalone
//! formatting. The constructed client's Debug reaches its session.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::time::Duration;

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
    concord_example_testserver::refused_url()
}

// Echo credentials in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(200, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(200, format!("{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::json(200, "{}").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from(
            format!("{{\"incomplete\":\"{echo}"),
        )])),
        _ => unreachable!(),
    };
    response
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

use concord_sdk_paypal::{CaptureRequest, Credentials, Error, OrderId, PayPal, TokenError};
use std::num::NonZeroUsize;

fn client(base: &str, id: &str, secret: &str, c: &mut Canaries) -> PayPal {
    let credentials = Credentials::new(id, secret).unwrap();
    c.assert_debug_absent("credentials", &credentials);
    let builder = PayPal::builder(http(), credentials)
        .base_url(base)
        .reads(concord_sdk_paypal::concord_retry::Policy::new(
            NonZeroUsize::new(1).unwrap(),
        ))
        .writes(concord_sdk_paypal::concord_retry::Policy::new(
            NonZeroUsize::new(1).unwrap(),
        ));
    c.assert_debug_absent("Builder", &builder);
    builder.build().unwrap()
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    match error {
        Error::Concord(e) => concord_error(c, e),
        Error::Token(e) => {
            c.assert_debug_absent("TokenError", e);
            c.assert_error_absent("TokenError and sources", e);
            if let TokenError::Refused {
                document: Some(document),
                ..
            } = e
            {
                c.assert_debug_absent("IdentityError", document);
            }
        }
        Error::Http(http) => {
            c.assert_debug_absent("HttpError", http);
            c.assert_display_absent("HttpError", http);
            c.assert_debug_absent("metadata", http.meta());
            match http.document() {
                Ok(document) => {
                    c.assert_debug_absent("ApiError", document);
                    c.assert_debug_absent("error details", document.details());
                }
                Err(e) => concord_error(c, e),
            }
        }
        Error::Uncertain(uncertain) => {
            c.assert_debug_absent("uncertain write", uncertain);
            c.assert_debug_absent("uncertain cause", uncertain.cause());
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("paypal");
    let id = c.issue();
    let secret = c.issue();
    let token = c.issue();
    // Exercise API failures separately from OAuth failures, so every API case really dispatches.
    for oauth_failure in [false, true] {
        for case in 0..7 {
            let echo = secret.clone();
            let access = token.clone();
            let mut server = Some(TestServer::start(move |r: &concord_example_testserver::Recorded| {
                if r.path() == "/v1/oauth2/token" {
                    let success = format!(r#"{{"access_token":"{access}","token_type":"Bearer","expires_in":3600}}"#);
                    let problem = format!(r#"{{"error":"invalid_client","error_description":"{echo}"}}"#);
                    reply(if oauth_failure { case } else { 0 }, &success, &problem, &echo)
                } else {
                    let problem = format!(r#"{{"name":"INVALID_REQUEST","message":"{echo}","details":[{{"issue":"INVALID_PARAMETER_VALUE","description":"{echo}","value":"{echo}"}}]}}"#);
                    reply(if oauth_failure { 0 } else { case }, include_str!("../fixtures/order-created.json"), &problem, &echo)
                }
            }).await);
            let base = if case == 6 && oauth_failure {
                refused()
            } else {
                server.as_ref().unwrap().url()
            };
            let api = client(&base, &id, &secret, &mut c);
            c.assert_debug_absent("client before token acquisition", &api);
            if case == 6 && !oauth_failure {
                // Populate the token cache, then close its bound origin: this tests API
                // connection refusal, independently of the OAuth refusal tested below.
                let warm = api
                    .order(OrderId::new("SYNTH0RDER1A2B3C").unwrap())
                    .get()
                    .send()
                    .await
                    .unwrap();
                c.assert_debug_absent("warm-up response", &warm);
                server.take().unwrap().shutdown().await;
            }
            let order = api.order(OrderId::new("SYNTH0RDER1A2B3C").unwrap());
            let read = order.get();
            let write = order.capture(CaptureRequest::new());
            c.assert_debug_absent("scope", &order);
            c.assert_debug_absent("read endpoint", &read);
            c.assert_debug_absent("write endpoint", &write);
            c.assert_debug_absent("read Prepared", &read.clone().prepare().unwrap());
            c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
            let result = read.send().await;
            c.assert_debug_absent("read Result", &result);
            assert_eq!(result.is_ok(), case == 0);
            if let Err(e) = &result {
                error(&mut c, e);
            }
            let result = write.send().await;
            c.assert_debug_absent("write Result", &result);
            assert_eq!(result.is_ok(), case == 0);
            if let Err(e) = &result {
                error(&mut c, e);
            }
            c.assert_debug_absent("client with populated token cache", &api);
            // A native send checks core errors independently; it carries no Authorization.
            if !oauth_failure {
                let result = receive(&mut c, order.get().prepare().unwrap()).await;
                c.assert_debug_absent("native Result", &result);
                if let Err(e) = &result {
                    concord_error(&mut c, e);
                }
            }
            if let Some(server) = server {
                if case == 0 {
                    c.assert_on_wire(&server.requests());
                }
                server.shutdown().await;
            }
        }
    }
    assert!(c.checked() >= 200);
    println!(
        "paypal: {} renderings; positive control passed",
        c.checked()
    );
}
