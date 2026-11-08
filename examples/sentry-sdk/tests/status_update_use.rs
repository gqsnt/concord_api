//! Consumer application for the status update: triage a page of issues against a loopback
//! fixture that applies each change (no provider calls; all data synthetic). Written from the
//! README's "Status updates" section and rustdoc.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_sdk_sentry::{
    Error, Ignore, Resume, SendPolicy, Sentry, Settings, StatusUpdate, http_client,
};
use concord_testserver::{Gate, Recorded, Reply, TestServer};
use serde_json::{Value, json};

const TOKEN: &str = "sntryu_synthetic_triage_token_0123456789abcdef";

fn issue_json(id: &str, status: &str) -> String {
    format!(
        r#"{{"id":"{id}","shortId":"BACKEND-{id}","title":"Synthetic failure {id}","status":"{status}",
        "substatus":"ongoing","project":{{"id":"4504","slug":"backend"}}}}"#
    )
}

/// A fixture Sentry that stores statuses, applies a PUT before answering, and can hold the
/// answer to one issue's update (applied, but unanswered until the gate opens).
fn triage_server(
    statuses: Arc<Mutex<HashMap<String, String>>>,
    slow: Gate,
) -> impl Fn(&Recorded) -> Reply {
    move |r: &Recorded| {
        let prefix = "/api/0/organizations/acme/issues/";
        let id = r.path().strip_prefix(prefix).unwrap().trim_end_matches('/');
        let mut statuses = statuses.lock().unwrap();
        match (r.method.as_str(), id) {
            ("GET", "") => {
                assert_eq!(r.query_all("query"), ["is:unresolved"]);
                let page: Vec<String> = ["11", "12", "13"]
                    .iter()
                    .map(|id| issue_json(id, &statuses[*id]))
                    .collect();
                Reply::json(200, format!("[{}]", page.join(","))).header(
                    "Link",
                    format!(
                        "<http://{h}{prefix}?&cursor=0:3:0>; rel=\"next\"; results=\"false\"; cursor=\"0:3:0\"",
                        h = r.host
                    ),
                )
            }
            ("GET", id) => Reply::json(200, issue_json(id, &statuses[id])),
            ("PUT", id) => {
                let change: Value = serde_json::from_slice(&r.body).unwrap();
                let status = match change["status"].as_str().unwrap() {
                    "resolvedInNextRelease" => "resolved",
                    other => other,
                };
                statuses.insert(id.to_owned(), status.to_owned());
                let reply = Reply::json(200, issue_json(id, status));
                if id == "13" {
                    reply.after(slow.clone())
                } else {
                    reply
                }
            }
            other => panic!("unexpected request {other:?}"),
        }
    }
}

#[tokio::test]
async fn triage_resolves_ignores_and_rereads_an_uncertain_update() {
    let statuses = Arc::new(Mutex::new(
        ["11", "12", "13"]
            .map(|id| (id.to_owned(), "unresolved".to_owned()))
            .into(),
    ));
    let slow = Gate::new();
    let server = TestServer::start(triage_server(statuses.clone(), slow.clone())).await;
    // A short scheduling deadline so a stuck update is reported instead of awaited.
    let settings = Settings {
        timeout: Duration::from_millis(300),
        ..Settings::default()
    };
    let api = Sentry::for_deployment(
        http_client().unwrap(),
        server.url(),
        TOKEN,
        SendPolicy::new(settings).unwrap(),
    )
    .unwrap();
    let organization = api.organization("acme");

    let issues = organization
        .issues()
        .set_query("is:unresolved")
        .pages(Resume::Start, NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .expect("issue list");
    assert_eq!(issues.len(), 3);

    // Resolve the first in the next release; the returned issue is the updated one.
    let resolved = organization
        .issue(&issues[0].id)
        .update_status(StatusUpdate::ResolvedInNextRelease)
        .send()
        .await
        .expect("resolve");
    assert_eq!(resolved.value.id, "11");
    assert_eq!(resolved.value.status, "resolved");

    // Ignore the second until it is seen 100 more times within an hour.
    let ignore = Ignore::until_seen(100, Some(60)).expect("documented window");
    let ignored = organization
        .issue(&issues[1].id)
        .update_status(StatusUpdate::Ignored(ignore))
        .send()
        .await
        .expect("ignore");
    assert_eq!(ignored.value.status, "ignored");

    // A mistyped window (8 days) is refused locally; nothing is sent.
    let before = server.requests().len();
    let refused = Ignore::until_seen(100, Some(8 * 24 * 60)).unwrap_err();
    assert!(matches!(refused, Error::InvalidUpdate(_)), "{refused:?}");
    assert_eq!(server.requests().len(), before);

    // The third update is applied but its answer does not arrive before the deadline.
    let third = organization.issue(&issues[2].id);
    let error = third
        .update_status(StatusUpdate::Resolved)
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Uncertain { .. }), "{error:?}");
    eprintln!("observed uncertain update: {error}");
    slow.open();
    // The caller re-reads instead of resending, and finds the change applied.
    let reread = third.get().send().await.expect("re-read");
    assert_eq!(reread.value.status, "resolved");

    let wire: Vec<(String, String, Option<Value>)> = server
        .requests()
        .into_iter()
        .map(|r| {
            let body = (!r.body.is_empty()).then(|| serde_json::from_slice(&r.body).unwrap());
            (r.method.clone(), r.path().to_owned(), body)
        })
        .collect();
    let issue = |id: &str| format!("/api/0/organizations/acme/issues/{id}/");
    assert_eq!(
        wire,
        [
            (
                "GET".into(),
                "/api/0/organizations/acme/issues/".into(),
                None
            ),
            (
                "PUT".into(),
                issue("11"),
                Some(json!({"status": "resolvedInNextRelease"}))
            ),
            (
                "PUT".into(),
                issue("12"),
                Some(json!({"status": "ignored", "ignoreCount": 100, "ignoreWindow": 60}))
            ),
            (
                "PUT".into(),
                issue("13"),
                Some(json!({"status": "resolved"}))
            ),
            ("GET".into(), issue("13"), None),
        ]
    );
    let statuses = statuses.lock().unwrap();
    assert_eq!(
        [&statuses["11"], &statuses["12"], &statuses["13"]],
        ["resolved", "ignored", "resolved"]
    );
}
