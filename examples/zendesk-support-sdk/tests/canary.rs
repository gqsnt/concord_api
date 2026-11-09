//! Secret: OAuth bearer token -> Authorization header; distinct canary in every credential
//! constructor input. Response headers and withheld provider descriptions/details echo it.
//! Show (read) and create (write) cover success, provider 4xx/5xx, malformed JSON, wrong media,
//! truncated body and refused connection. Native Prepared sends exercise concord::Error.
//! Excludes native reqwest, *_unredacted accessors, ticket models and idempotency keys,
//! which are documented clear caller data. No wire exemptions.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Reply, TestServer};
use std::time::Duration;

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

use concord_sdk_zendesk_support::{
    Error, IdempotencyKey, NewTicket, SendPolicy, Zendesk, ZendeskConfig,
};
use std::{net::SocketAddr, num::NonZeroUsize};

fn api(address: SocketAddr, token: &str) -> Zendesk {
    let http = concord::reqwest::Client::builder()
        .no_proxy()
        .retry(concord::reqwest::retry::never())
        .redirect(concord::reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2));
    Zendesk::for_fixture(http, "acme", token, address)
        .unwrap()
        .set_policy(SendPolicy::default().read_attempts(NonZeroUsize::new(1).unwrap()))
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    if let Some(http) = error.http() {
        c.assert_debug_absent("HTTP failure", http);
        c.assert_debug_absent("HTTP metadata", http.meta());
        match http.document() {
            Ok(problem) => c.assert_debug_absent("provider document", problem),
            Err(e) => concord_error(c, e),
        }
    }
    if let Some(uncertain) = error.uncertain() {
        c.assert_debug_absent("uncertain write", uncertain);
        c.assert_debug_absent("uncertain cause", uncertain.cause());
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("zendesk-support");
    let token = c.issue();
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |_| reply(case, include_str!("../fixtures/ticket-103.json"),
            &format!(r#"{{"error":"RecordInvalid","description":"{echo}","details":{{"base":[{{"description":"{echo}"}}]}}}}"#), &echo)).await;
        let address = if case == 6 {
            concord_example_testserver::refused_addr()
        } else {
            server.addr()
        };
        let api = api(address, &token);
        let config = ZendeskConfig {
            subdomain: "acme".into(),
            authorization: concord::reqwest::header::HeaderValue::from_str(&format!(
                "Bearer {token}"
            ))
            .unwrap(),
            policy: SendPolicy::default(),
        };
        c.assert_debug_absent("config", &config);
        c.assert_debug_absent("policy", &config.policy);
        let ticket = api.ticket(103);
        let tickets = api.tickets();
        let read = ticket.show();
        let new = NewTicket::new("ordinary caller content");
        c.assert_debug_absent("new ticket", &new);
        let write = tickets.create(new, IdempotencyKey::new("canary-create").unwrap());
        c.assert_debug_absent("client", &api);
        c.assert_debug_absent("ticket scope", &ticket);
        c.assert_debug_absent("tickets scope", &tickets);
        c.assert_debug_absent("read endpoint", &read);
        c.assert_debug_absent("write endpoint", &write);
        c.assert_debug_absent("write Prepared", &write.clone().prepare().unwrap());
        let prepared = read.clone().prepare().unwrap();
        c.assert_debug_absent("read Prepared", &prepared);
        let native = receive(&mut c, prepared).await;
        c.assert_debug_absent("native Result", &native);
        assert_eq!(native.is_ok(), case == 0);
        if let Err(e) = &native {
            concord_error(&mut c, e);
        }
        let read_result = read.send().await;
        let write_result = write.send().await;
        c.assert_debug_absent("read Result", &read_result);
        c.assert_debug_absent("write Result", &write_result);
        assert_eq!(read_result.is_ok(), case == 0);
        assert_eq!(write_result.is_ok(), case == 0);
        if let Err(e) = read_result {
            error(&mut c, &e);
        }
        if let Err(e) = write_result {
            error(&mut c, &e);
        }
        if case == 0 {
            c.assert_on_wire(&server.requests());
        }
        server.shutdown().await;
    }
    assert!(c.checked() >= 200);
    println!(
        "zendesk-support: {} renderings; positive control passed",
        c.checked()
    );
}
