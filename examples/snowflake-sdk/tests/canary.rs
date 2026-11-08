//! Secrets: the caller-issued token (`Client::new`, `src/client.rs:80-121`, "marked sensitive;
//! Debug never includes it"; `#[sensitive] header(AUTHORIZATION)`, `src/api.rs:34`, sent as
//! `Bearer`), and the SQL text (`Statement::statement`; `Statement` has no `Debug`, provider
//! errors "may contain SQL" and are not printed, `src/error.rs:1,35`). Response-only values the
//! SDK withholds, each with its own canary and reaching the decoder through a fixture: the
//! provider `message` and error body (`ServerMessage`/`ServerError` Debug print code, SQLSTATE
//! and body length only, `src/model.rs:154`, `src/error.rs:50`), the opaque query of the
//! returned `statementStatusUrl` and of a partition `Link` target ("Debug never exposes opaque
//! URL queries", `src/links.rs:10`) and a result cell (`ResultSet`/`Partition` Debug print row
//! counts, `src/model.rs:209`, `src/partitions.rs:218`). The two opaque URLs are followed by the
//! SDK, so they travel too. The server also echoes the token into headers and into every
//! malformed, wrong-media and truncated body. Reads (`Query::status`, `Query::poll`,
//! `PartitionPlan::fetch`) and writes (`Client::submit`, `Query::cancel`) cover the seven
//! scenarios; the success case also runs a bounded partition traversal and its report. For the
//! refused case the submit targets a closed port and the bound queries are used after the
//! server shut down (error path only). A token refused by `Client::new` is checked too.
//! Token classification (`TokenType`) only changes a public header value: one OAuth client is
//! used. No signing secret exists (a key-pair JWT is signed by the caller). Not covered: native
//! reqwest values; the generated `SqlApi`/`Prepared` are crate-private; explicit clear fields
//! (`ServerError::body`, `ServerMessage::message`/`statement_status_url`, `Partition::rows`,
//! `ResultSet::data`). Handles, request IDs, session names and result metadata are printable
//! by design. Wire exemptions: the provider `message` and the result cell are response-only
//! (decoded, asserted through the explicit fields, never sent by the client).

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply as WireReply, TestServer};
use concord_sdk_snowflake::{
    Client, ClientOptions, Error, FirstResult, LocalStop, PollOutcome, PollPolicy, Query,
    RequestContext, RequestId, Statement, StatementState, TokenType,
};
use std::{num::NonZeroUsize, time::Duration};

const A: &str = "00000000-0000-4000-8000-00000000000a";
const B: &str = "00000000-0000-4000-8000-00000000000b";
const SETUP_A: &str = "00000000-0000-4000-8000-0000000000a1";
const SETUP_B: &str = "00000000-0000-4000-8000-0000000000b1";
const SUBMIT: &str = "00000000-0000-4000-8000-000000000101";
const CANCEL: &str = "00000000-0000-4000-8000-000000000102";

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn options() -> ClientOptions {
    ClientOptions {
        request_timeout: Duration::from_secs(2),
        ..ClientOptions::default()
    }
}

fn id(s: &str) -> RequestId {
    RequestId::new(s).unwrap()
}

struct Echo {
    token: String,
    message: String,
    status_query: String,
    link_query: String,
    cell: String,
}

impl Echo {
    fn pending(&self, handle: &str) -> String {
        format!(
            r#"{{"code":"333334","message":"{}","statementHandle":"{handle}","statementStatusUrl":"/api/v2/statements/{handle}?requestId=00000000-0000-4000-8000-000000000201&opaque={}"}}"#,
            self.message, self.status_query
        )
    }
    fn complete(&self, handle: &str) -> String {
        format!(
            r#"{{"code":"090001","message":"{}","sqlState":"00000","statementHandle":"{handle}","statementStatusUrl":"/api/v2/statements/{handle}","resultSetMetaData":{{"numRows":3,"format":"jsonv2","rowType":[{{"name":"ID","type":"fixed"}},{{"name":"LABEL","type":"text"}}],"partitionInfo":[{{"rowCount":2,"uncompressedSize":32}},{{"rowCount":1,"uncompressedSize":24}}]}},"data":[["1","{}"],["2",null]]}}"#,
            self.message, self.cell
        )
    }
    fn link(&self, handle: &str) -> String {
        format!(
            r#"</api/v2/statements/{handle}?partition=1&requestId=00000000-0000-4000-8000-000000000301&opaque={}>; rel="next""#,
            self.link_query
        )
    }
    fn problem(&self, handle: &str) -> String {
        format!(
            r#"{{"code":"002140","message":"{}","sqlState":"42601","statementHandle":"{handle}","detail":"{}"}}"#,
            self.message, self.token
        )
    }
}

// Echo the token in headers and in diagnostics whose formatting is withheld.
fn reply(case: usize, success: WireReply, problem: &str, echo: &str) -> WireReply {
    let response = match case {
        0 | 6 => success,
        1 => WireReply::json(422, problem),
        2 => WireReply::json(500, problem),
        3 => WireReply::json(200, format!("{{invalid {echo}")),
        4 => WireReply::text(200, "text/html", echo),
        5 => WireReply::json(200, "{}").with_body(Body::FailAfter(vec![
            concord::bytes::Bytes::from(format!("{{\"incomplete\":\"{echo}")),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-snowflake-request-id", echo)
        .header("set-cookie", echo)
}

fn respond(case: usize, e: &Echo, r: &Recorded) -> WireReply {
    let handle = if r.path().contains(B) { B } else { A };
    let partition = !r.query_all("partition").is_empty();
    match (r.method.as_str(), r.path().ends_with("/cancel"), partition) {
        ("POST", false, _) => match r.query_all("requestId").first().map(String::as_str) {
            Some(SETUP_A) => WireReply::json(202, e.pending(A)),
            Some(SETUP_B) => WireReply::json(202, e.pending(B)),
            _ => reply(
                case,
                WireReply::json(202, e.pending(A)),
                &e.problem(A),
                &e.token,
            ),
        },
        ("POST", true, _) => {
            let ack = format!(
                r#"{{"code":"000604","message":"{}","sqlState":"57014","statementHandle":"{A}"}}"#,
                e.message
            );
            reply(case, WireReply::json(200, ack), &e.problem(A), &e.token)
        }
        (_, _, true) => {
            let rows = WireReply::json(200, format!(r#"{{"data":[["3","{}"]]}}"#, e.cell));
            if handle == B {
                reply(case, rows, &e.problem(B), &e.token)
            } else {
                rows
            }
        }
        _ => {
            let complete = WireReply::json(200, e.complete(handle)).header("link", e.link(handle));
            if handle == B {
                complete
            } else {
                reply(case, complete, &e.problem(A), &e.token)
            }
        }
    }
}

fn concord_error(c: &mut Canaries, error: &concord::Error) {
    c.assert_debug_absent("concord::Error", error);
    c.assert_error_absent("concord::Error and sources", error);
    if let Some(meta) = error.head_meta() {
        c.assert_debug_absent("error response metadata", &meta);
    }
}

fn error(c: &mut Canaries, error: &Error) {
    c.assert_debug_absent("SDK error", error);
    c.assert_error_absent("SDK error and sources", error);
    c.assert_debug_absent("error metadata", &error.response_meta());
    match error {
        Error::Concord { source, .. } => concord_error(c, source),
        Error::Server(e) => {
            c.assert_debug_absent("server error", e);
            c.assert_debug_absent("server document", &e.document);
            c.assert_debug_absent("server metadata", &e.meta);
            if let Some(e) = &e.read_error {
                concord_error(c, e);
            }
            if let Some(e) = &e.decode_error {
                c.assert_debug_absent("server decode error", e);
                c.assert_display_absent("server decode error", e);
            }
        }
        Error::PartitionBody { source, .. } => {
            c.assert_debug_absent("partition body error", source);
            c.assert_error_absent("partition body error", source);
        }
        _ => {}
    }
}

fn first(c: &mut Canaries, first: &FirstResult) {
    c.assert_debug_absent("first result", first);
    c.assert_debug_absent("first reply", first.response());
    c.assert_debug_absent("result set", &first.response().value);
    c.assert_debug_absent("result status", &first.response().value.status);
    c.assert_debug_absent("result metadata", &first.response().meta);
    c.assert_debug_absent("partition plan", &first.plan());
    c.assert_debug_absent("plan query", first.plan().query());
}

async fn bound(api: &Client, setup: &str) -> Query {
    let context = RequestContext {
        submission_id: id(setup),
    };
    api.submit(context, Statement::new("select 1"))
        .await
        .unwrap()
        .query
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("snowflake");
    let token = c.issue();
    let sql = format!("select ID, LABEL from DEMO where LABEL = '{}'", c.issue());
    let echo = Echo {
        token: token.clone(),
        message: c.issue(),
        status_query: c.issue(),
        link_query: c.issue(),
        cell: c.issue(),
    };
    let echo = std::sync::Arc::new(echo);
    let policy = PollPolicy {
        max_attempts: NonZeroUsize::new(1).unwrap(),
        interval: Duration::from_millis(10),
        max_elapsed: Duration::from_secs(5),
    };
    c.assert_debug_absent("options", &options());
    c.assert_debug_absent("poll policy", &policy);
    for case in 0..7 {
        let e = echo.clone();
        let server = TestServer::start(move |r: &Recorded| respond(case, &e, r)).await;
        let api = Client::new(&server.url(), &token, TokenType::OAuth, options()).unwrap();
        c.assert_debug_absent("client", &api);
        let query = bound(&api, SETUP_A).await;
        let healthy = bound(&api, SETUP_B).await;
        // Partition zero and the plan of a healthy query, so `fetch` meets each scenario.
        let plan_b = match healthy.status().await.unwrap() {
            StatementState::Complete(f) => {
                first(&mut c, &f);
                f.plan()
            }
            other => panic!("expected completion: {other:?}"),
        };
        c.assert_debug_absent("query", &query);
        c.assert_debug_absent("query context", query.context());
        let mut statement = Statement::new(&sql);
        statement.session.warehouse = Some("TEST_WH".into());
        let base = if case == 6 { refused() } else { server.url() };
        let writer = Client::new(&base, &token, TokenType::OAuth, options()).unwrap();
        let mut server = Some(server);
        if case == 6 {
            server.take().unwrap().shutdown().await;
        }
        let context = RequestContext {
            submission_id: id(SUBMIT),
        };
        let submitted = writer.submit(context, statement).await;
        c.assert_debug_absent("submit Result", &submitted);
        assert_eq!(submitted.is_ok(), case == 0);
        match &submitted {
            Ok(s) => {
                c.assert_debug_absent("submission query", &s.query);
                c.assert_debug_absent("submission state", &s.state);
            }
            Err(e) => error(&mut c, e),
        }
        let status = query.status().await;
        c.assert_debug_absent("status Result", &status);
        assert_eq!(status.is_ok(), case == 0);
        match status {
            Ok(StatementState::Complete(f)) => {
                first(&mut c, &f);
                let partition = f.plan().fetch(1).await.unwrap();
                c.assert_debug_absent("linked partition", &partition);
                c.assert_debug_absent("partition metadata", &partition.meta);
                assert_eq!(partition.rows[0][1].as_deref(), Some(echo.cell.as_str()));
                let mut traversal = f.bounded_partitions(
                    NonZeroUsize::new(4).unwrap(),
                    NonZeroUsize::new(2).unwrap(),
                );
                while let Some(item) = traversal.next().await {
                    c.assert_debug_absent("traversal item", &item);
                }
                let report = traversal.finish();
                c.assert_debug_absent("partition report", &report);
                assert!(report.is_complete());
            }
            Ok(other) => panic!("expected completion: {other:?}"),
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    // Withholding keeps diagnosis: status, code, SQLSTATE and operation.
                    assert!(e.to_string().contains("422"), "{e}");
                    let Error::Server(server_error) = &e else {
                        panic!("{e:?}")
                    };
                    let document = server_error.document.as_ref().unwrap();
                    assert_eq!(document.code, "002140");
                    assert!(format!("{server_error:?}").contains("42601"));
                    let operation = e.response_meta().unwrap().operation().unwrap();
                    assert!(operation.name().contains("status"), "{}", operation.name());
                }
            }
        }
        let polled = query.poll(policy, &LocalStop::default()).await;
        c.assert_debug_absent("poll Result", &polled);
        assert_eq!(matches!(polled, Ok(PollOutcome::Terminal(_))), case == 1);
        match &polled {
            Ok(PollOutcome::Terminal(e)) => {
                assert_eq!(case, 1);
                c.assert_debug_absent("terminal server error", e);
            }
            Ok(outcome) => {
                assert_eq!(case, 0);
                c.assert_debug_absent("poll outcome", outcome);
            }
            Err(e) => error(&mut c, e),
        }
        let partition = plan_b.fetch(1).await;
        c.assert_debug_absent("partition Result", &partition);
        assert_eq!(partition.is_ok(), case == 0);
        if let Err(e) = &partition {
            error(&mut c, e);
        }
        let cancelled = query.cancel(id(CANCEL)).await;
        c.assert_debug_absent("cancel Result", &cancelled);
        assert_eq!(cancelled.is_ok(), case == 0);
        match &cancelled {
            Ok(ack) => {
                c.assert_debug_absent("cancel acknowledgement", &ack.value);
                assert_eq!(ack.value.message, echo.message);
                c.assert_debug_absent("cancel metadata", &ack.meta);
            }
            Err(e) => error(&mut c, e),
        }
        if case == 0 {
            let requests = server.as_ref().unwrap().requests();
            // Provider text and result cells only come back from the server.
            c.assert_on_wire_except(&requests, &[&echo.message, &echo.cell]);
            for r in &requests {
                assert_eq!(
                    r.header("authorization"),
                    Some(format!("Bearer {token}").as_str())
                );
            }
        }
    }
    assert!(c.checked() >= 250);
    println!(
        "snowflake: {} renderings; positive control passed (2 response-only canaries exempt)",
        c.checked()
    );
}

#[test]
fn refused_token_never_prints() {
    let mut c = Canaries::new("snowflake");
    let token = format!("{} {}", c.issue(), c.issue());
    let e = Client::new(
        "http://127.0.0.1:9",
        &token,
        TokenType::KeyPairJwt,
        options(),
    )
    .unwrap_err();
    error(&mut c, &e);
    assert!(e.to_string().contains("token"));
}
