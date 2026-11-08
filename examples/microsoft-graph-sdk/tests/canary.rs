//! Secrets/withheld values: bearer token (src/client.rs:84,97; src/api.rs:8), opaque
//! NextLink/DeltaLink query tokens (src/links.rs:65-98), and AuthContext tenant/principal
//! labels (src/client.rs:17-54), all distinct. Links are decoded from fixtures, followed,
//! restored and resumed. Provider code/message/detail/header echoes enter withheld renderers.
//! Seven scenarios on groups; no writes in this SDK. Excludes native reqwest, clear model
//! values, context accessors, link url()/to_json(), explicit unredacted accessors. Context
//! label canaries are wire-exempt: local storage bindings, never authentication or HTTP input.
//! The generated API/config/endpoints and Prepared factories are private, so not constructed.

use concord_check_canary::Canaries;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};

fn refused() -> String {
    concord_example_testserver::refused_url()
}

fn reply(case: usize, media: &str, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::text(200, media, success),
        1 => Reply::text(400, media, problem),
        2 => Reply::text(503, media, problem),
        3 => Reply::text(200, media, format!("<{{invalid {echo}")),
        4 => Reply::text(200, "text/html", echo),
        5 => Reply::text(200, media, "").with_body(Body::FailAfter(vec![
            format!("<{{incomplete {echo}").into(),
        ])),
        _ => unreachable!(),
    };
    response
        .header("x-private", echo)
        .header("set-cookie", echo)
}

fn concord_error(c: &mut Canaries, e: &concord::Error) {
    c.assert_debug_absent("Concord error", e);
    c.assert_error_absent("Concord error/source chain", e);
    c.assert_debug_absent("error kind", &e.kind());
    c.assert_debug_absent("error head", &e.head());
    c.assert_debug_absent("error metadata", &e.head_meta());
}

use concord_sdk_microsoft_graph::{AuthContext, ErrorBody, ErrorClass, GraphClient, GraphError};

fn error(c: &mut Canaries, e: &GraphError) {
    c.assert_debug_absent("GraphError", e);
    c.assert_error_absent("GraphError/source chain", e);
    c.assert_debug_absent("error metadata", &e.meta());
    c.assert_debug_absent("error class", &e.class());
    match e {
        GraphError::Concord { error, .. } => concord_error(c, error),
        GraphError::Provider { body, .. } => {
            c.assert_debug_absent("error body", body);
            match body {
                ErrorBody::Parsed(p) => {
                    c.assert_debug_absent("provider error", p);
                    c.assert_debug_absent("nested detail", &p.inner_error);
                    c.assert_debug_absent("provider details", &p.details);
                }
                ErrorBody::Unavailable(e) => concord_error(c, e),
                ErrorBody::Malformed => {}
            }
        }
        _ => {}
    }
}

#[tokio::test]
async fn secrets_never_print() {
    let mut c = Canaries::new("microsoft-graph");
    let token = c.issue();
    let tenant = c.issue();
    let principal = c.issue();
    let next_token = c.issue();
    let delta_token = c.issue();
    let context = AuthContext::new(&tenant, &principal).unwrap();
    c.assert_debug_absent("authorization context", &context);
    for case in 0..7 {
        let (echo, next, delta) = (token.clone(), next_token.clone(), delta_token.clone());
        let server = TestServer::start(move |r: &Recorded| {
            let success = if r.path() == "/v1.0/users/delta" {
                let link = if r.query_pairs().is_empty() {
                    format!(r#""@odata.nextLink":"http://{}/v1.0/users/delta?$skiptoken={next}""#, r.host)
                } else {
                    format!(r#""@odata.deltaLink":"http://{}/v1.0/users/delta?$deltatoken={delta}""#, r.host)
                };
                format!(r#"{{"value":[],{link}}}"#)
            } else { r#"{"value":[]}"#.into() };
            let problem = format!(r#"{{"error":{{"code":"{echo}","message":"{echo}","innerError":{{"code":"{echo}","message":"{echo}","request-id":"{echo}","innerError":{{"message":"{echo}"}}}},"details":[{{"message":"{echo}"}}],"custom":"{echo}"}}}}"#);
            reply(case, "application/json", &success, &problem, &echo)
        }).await;
        let base = format!("{}/v1.0", if case == 6 { refused() } else { server.url() });
        let api = GraphClient::for_loopback(context.clone(), &token, &base).unwrap();
        c.assert_debug_absent("client", &api);
        let listing = api.groups(&[]).unwrap();
        c.assert_debug_absent("groups listing", &listing);
        let result = listing.fetch(None).await;
        c.assert_debug_absent("page Result", &result);
        assert_eq!(result.is_ok(), case == 0);
        match result {
            Ok(page) => {
                c.assert_debug_absent("page", &page);
                c.assert_debug_absent("page metadata", &page.meta);
                c.assert_debug_absent("HTTP metadata", &page.meta.response);
            }
            Err(e) => {
                error(&mut c, &e);
                if case == 1 {
                    assert_eq!(e.class(), ErrorClass::InvalidRequest);
                    assert_eq!(e.meta().unwrap().status().as_u16(), 400);
                    assert!(e.to_string().contains("400"));
                    assert_eq!(e.provider().unwrap().message, token);
                }
            }
        }
        let mut report_walk = listing.pages();
        let first = report_walk.next_page().await;
        c.assert_debug_absent("report traversal page", &first);
        if let Err(e) = first {
            c.assert_error_absent("report PagesError", &e);
        }
        c.assert_debug_absent("traversal checkpoint", &report_walk.checkpoint());
        c.assert_debug_absent("traversal progress", &report_walk.progress());
        c.assert_debug_absent("traversal report", &report_walk.finish());
        let walk = listing.pages();
        c.assert_debug_absent("group traversal", &walk);
        let complete = walk.collect_complete().await;
        c.assert_debug_absent("complete traversal Result", &complete);
        if let Err(incomplete) = complete {
            c.assert_debug_absent("incomplete report", &incomplete);
            c.assert_error_absent("incomplete report/source chain", &incomplete);
            c.assert_error_absent("PagesError", &incomplete.error);
        }
        if case == 0 {
            let users = api.users_delta(&[]).unwrap();
            c.assert_debug_absent("users listing", &users);
            let first = users.fetch(None).await.unwrap();
            c.assert_debug_absent("next-link page", &first);
            let next = first.next.unwrap();
            c.assert_debug_absent("NextLink", &next);
            let next = users.restore_next(&next.to_json()).unwrap();
            c.assert_debug_absent("restored NextLink", &next);
            let final_page = users.fetch(Some(&next)).await.unwrap();
            c.assert_debug_absent("delta-link page", &final_page);
            c.assert_debug_absent("final page metadata", &final_page.meta);
            let delta = final_page.meta.delta_link.unwrap();
            c.assert_debug_absent("DeltaLink", &delta);
            let delta = api.restore_delta(&delta.to_json()).unwrap();
            c.assert_debug_absent("restored DeltaLink", &delta);
            let resumed = api.resume_users(&delta).unwrap();
            c.assert_debug_absent("resumed listing", &resumed);
            c.assert_debug_absent("resumed page", &resumed.fetch(None).await.unwrap());
            let members = api.direct_members("ordinary-group", &[]).unwrap();
            c.assert_debug_absent("members listing", &members);
            c.assert_debug_absent("members page", &members.fetch(None).await.unwrap());
            c.assert_on_wire_except(&server.requests(), &[&tenant, &principal]);
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
    let e = GraphClient::for_loopback(context, format!("{token}\n"), "http://127.0.0.1:1/v1.0")
        .unwrap_err();
    error(&mut c, &e);
    assert!(c.checked() >= 200);
    println!(
        "microsoft-graph: {} renderings; positive control passed (local context labels exempt)",
        c.checked()
    );
}
