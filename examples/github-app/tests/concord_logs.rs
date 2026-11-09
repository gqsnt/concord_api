//! J1 contract (from the design packet, now compiled and run against the spike).
//! One decoder execution per case; body handling must not emit another event.

mod log_capture;

use std::time::Duration;

use concord::reqwest;
use concord_check_canary::Canaries;
use concord_example_github_sdk::{GitHubConfig, GitHubContext, RepoRef};
use concord_example_testserver::{Body, Gate, Recorded, Reply, TestServer};
use log_capture::Capture;

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn seven_cases_have_one_static_event_and_no_secret() {
    let mut canaries = Canaries::new("j1-github");
    let token = canaries.issue();
    // Ordinary values are deliberately canaries too: automatic logs cannot rely on annotation.
    let owner = "CANARYJ1GITHUBOWNERZ";
    let name = "CANARYJ1GITHUBREPOZ";
    let assignee = "CANARYJ1GITHUBQUERYZ";
    for case in 0..7 {
        let echo = token.clone();
        let server = TestServer::start(move |_: &Recorded| {
            let reply = match case {
                0 | 6 => Reply::json(200, "[]"),
                1 => Reply::json(401, format!(r#"{{"message":"{echo}"}}"#)),
                2 => Reply::json(503, format!(r#"{{"message":"{echo}"}}"#)),
                3 => Reply::json(200, format!("[invalid {echo}")),
                4 => Reply::text(200, "text/html", &echo),
                5 => Reply::json(200, "").with_body(Body::FailAfter(vec![
                    format!(r#"[{{"body":"{echo}"#).into(),
                ])),
                _ => unreachable!(),
            };
            // Even a provider correlation header is NOT an automatic J1 field (J2 is separate).
            reply
                .header("x-private", &echo)
                .header("x-github-request-id", &echo)
        })
        .await;
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let origin = if case == 6 {
            concord_example_testserver::refused_url()
        } else {
            server.url()
        };
        let github = GitHubContext::new(
            http(),
            GitHubConfig {
                origin: concord::BaseUrl::from_static("https://api.github.com"),
                token: token.clone(),
            },
        )
        .with_base_url(origin)
        .unwrap();
        let repo = github.repo(RepoRef::new(owner, name));
        let prepared = repo
            .handle()
            .issues()
            .set_assignee(assignee)
            .prepare()
            .unwrap();
        // Summary is opt-in: ordinary path values remain visible; annotated token does not.
        canaries.assert_display_absent("prepared summary", &prepared.summary());
        assert!(prepared.summary().to_string().contains(owner));
        capture.assert_empty();
        let result = match prepared.receive().await {
            Ok(received) => {
                canaries.assert_display_absent("received summary", &received.summary());
                capture.assert_call(
                    "GitHub.repo.issues",
                    "GET",
                    "repos/{owner}/{name}/issues",
                    Some(match case {
                        1 => 401,
                        2 => 503,
                        _ => 200,
                    }),
                );
                received.decode().await
            }
            Err(error) => Err(error),
        };
        assert_eq!(result.is_ok(), case == 0);
        if let Err(error) = result {
            canaries.assert_error_absent("Concord error", &error);
            canaries.assert_display_absent("error summary", &error.summary());
        }
        capture.assert_call(
            "GitHub.repo.issues",
            "GET",
            "repos/{owner}/{name}/issues",
            if case == 6 {
                None
            } else {
                Some(match case {
                    1 => 401,
                    2 => 503,
                    _ => 200,
                })
            },
        );
        canaries.assert_absent("all raw Concord events and spans", &capture.raw());
        capture.assert_absent(&[owner, name, assignee]);
        if case == 0 {
            let requests = server.requests();
            assert_eq!(requests.len(), 1);
            canaries.assert_on_wire(&requests);
            for input in [owner, name, assignee] {
                assert!(requests[0].target.contains(input));
            }
        }
        if case == 6 {
            assert!(server.requests().is_empty());
        }
        server.shutdown().await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn build_cancel_and_native_execution_do_not_fabricate_head_events() {
    let gate = Gate::new();
    let held = gate.clone();
    let server = TestServer::start(move |_| Reply::json(200, "[]").after(held.clone())).await;
    let capture = Capture::default();
    let dispatch = capture.dispatch();
    let _subscriber = tracing::dispatcher::set_default(&dispatch);
    let github = GitHubContext::new(
        http(),
        GitHubConfig {
            origin: concord::BaseUrl::from_static("https://api.github.com"),
            token: "synthetic-token".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let repo = github.repo(RepoRef::new("octo-org", "octo-repo"));

    let prepared = repo.handle().issues().prepare().unwrap();
    assert!(prepared.follow("http://foreign.invalid/secret").is_err());
    capture.assert_empty(); // Build/NotSent is an error to the caller, not a completed attempt.

    let prepared = repo.handle().issues().prepare().unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(20), prepared.receive())
            .await
            .is_err()
    );
    capture.assert_empty(); // No response head; cancellation has no completion event.

    gate.open();
    let (client, request, decoder) = repo.handle().issues().prepare().unwrap().into_parts();
    let result = client.execute(request).await;
    decoder.finish(result).await.unwrap();
    capture.assert_empty(); // Direct reqwest execution is an explicit exit; finish only decodes.
    server.shutdown().await;
}

concord::api! {
    pub client DiagnosticProbe(#[sensitive] tenant: String, #[sensitive] key: String) {
        base "https://example.invalid/prefix";
        subdomain "{tenant}";
        GET read(query(..) pairs?: Vec<(String, String)>) { path "{client.key}"; }
    }
}

#[test]
fn summary_redacts_declared_hosts_paths_and_all_dynamic_query_pairs() {
    let mut canaries = Canaries::new("j1-summary");
    let key = canaries.issue();
    let dynamic_name = canaries.issue();
    let dynamic_value = canaries.issue();
    let host = "canaryj1summaryhostz"; // DNS labels are lowercase; assert the exact wire spelling.
    let api = DiagnosticProbe::new(
        http(),
        DiagnosticProbeConfig {
            tenant: host.into(),
            key,
        },
    );
    let prepared = api
        .read()
        .set_pairs(vec![
            (dynamic_name.clone(), dynamic_value.clone()),
            (dynamic_name.clone(), "second-value".into()), // duplicate keys must not bypass withholding
        ])
        .prepare()
        .unwrap();
    let url = prepared.request().url().as_str();
    assert!(url.contains(host) && url.contains(&dynamic_value) && url.contains(&dynamic_name));
    // Positive control for this build-only oracle: all inputs occur in the native target.
    for canary in canaries.issued() {
        assert!(url.contains(canary));
    }
    let summary = prepared.summary().to_string();
    canaries.assert_absent("dynamic query summary", &summary);
    canaries.assert_debug_absent("Prepared using the same redaction", &prepared);
    assert!(!summary.contains(host));
    assert!(summary.contains("<redacted>") && summary.ends_with("?<withheld>"));
    let (_client, mut request, decoder) = prepared.into_parts();
    request.url_mut().set_path("/CANARYJ1NATIVEPATHZ");
    request
        .url_mut()
        .set_query(Some("CANARYJ1NATIVENAMEZ=CANARYJ1NATIVEVALUEZ"));
    *request.method_mut() = reqwest::Method::from_bytes(b"CANARYJ1NATIVEMETHODZ").unwrap();
    request.headers_mut().insert(
        "x-canaryj1nativenamez",
        "CANARYJ1NATIVEHEADERZ".parse().unwrap(),
    );
    let native = decoder.summary().to_string();
    assert_eq!(native, "DiagnosticProbe.read: GET <withheld>");
    for canary in [
        host,
        "CANARYJ1NATIVEPATHZ",
        "CANARYJ1NATIVENAMEZ",
        "CANARYJ1NATIVEVALUEZ",
        "CANARYJ1NATIVEMETHODZ",
        "CANARYJ1NATIVEHEADERZ",
        "x-canaryj1nativenamez",
    ] {
        assert!(!native.contains(canary));
    }
}

concord::api! {
    pub client LogProbe(#[sensitive] tenant: String, #[sensitive] secret: String) {
        base "http://probe.invalid/prefix";
        subdomain "{tenant}";
        headers { "x-probe-key" = "{secret}"; }
        scope vault(#[sensitive] vault: String) {
            path "vaults/{vault}";
            GET item(
                item: String,
                query("q") q: String,
                query(..) pairs?: Vec<(String, String)>,
            ) {
                path "items/{item}";
            }
        }
    }
}

/// J1 guarantee over every declared placement at once: a sensitive host label, header and path
/// segment, an ordinary path hole, an ordinary query value and dynamic `query(..)` pairs are all
/// canaries, for success, 4xx, 5xx, a refused connection and a call cancelled before its head.
#[tokio::test(flavor = "current_thread")]
async fn every_placement_is_a_canary_and_none_reaches_the_subscriber() {
    let mut canaries = Canaries::new("j1-probe");
    let secret = canaries.issue();
    let vault = canaries.issue();
    let item = canaries.issue();
    let q = canaries.issue();
    let pair_name = canaries.issue();
    let pair_value = canaries.issue();
    // DNS lower-cases a host label: this canary is checked case-insensitively below.
    let tenant = "canaryj1probetenantz";
    let mut all: Vec<String> = canaries.issued().to_vec();
    all.push(tenant.into());
    for case in 0..5 {
        let gate = Gate::new();
        let held = gate.clone();
        let server = TestServer::start(move |_: &Recorded| match case {
            0 => Reply::new(204),
            1 => Reply::text(404, "text/plain", "missing"),
            2 => Reply::text(500, "text/plain", "broken"),
            _ => Reply::new(204).after(held.clone()),
        })
        .await;
        let addr = if case == 3 {
            concord_example_testserver::refused_addr()
        } else {
            server.addr()
        };
        let http = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
            .resolve(&format!("{tenant}.probe.invalid"), addr)
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        let api = LogProbe::new(
            http,
            LogProbeConfig {
                tenant: tenant.into(),
                secret: secret.clone(),
            },
        )
        .with_base_url(format!("http://probe.invalid:{}/prefix", addr.port()))
        .unwrap();
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let prepared = api
            .vault(vault.clone())
            .item(item.clone(), q.clone())
            .set_pairs(vec![(pair_name.clone(), pair_value.clone())])
            .prepare()
            .unwrap();
        // The opt-in summary: ordinary path values print, sensitive placements and pairs do not.
        let summary = prepared.summary().to_string();
        assert!(
            summary.contains(&item) && summary.ends_with("?<withheld>"),
            "{summary}"
        );
        for hidden in [&secret, &vault, &q, &pair_name, &pair_value] {
            assert!(!summary.contains(hidden.as_str()), "{summary}");
        }
        assert!(!summary.contains(tenant), "{summary}");
        if case == 4 {
            let cancelled =
                tokio::time::timeout(Duration::from_millis(50), prepared.receive()).await;
            assert!(cancelled.is_err());
            capture.assert_empty(); // Dropped before its head: no event at all.
            gate.open();
        } else {
            let received = prepared.receive().await;
            assert_eq!(received.is_ok(), case != 3);
            let status = match case {
                0 => Some(204),
                1 => Some(404),
                2 => Some(500),
                _ => None,
            };
            capture.assert_call(
                "LogProbe.vault.item",
                "GET",
                "prefix/vaults/{vault}/items/{item}",
                status,
            );
        }
        let raw = capture.raw().to_lowercase();
        for canary in &all {
            assert!(
                !raw.contains(&canary.to_lowercase()),
                "canary {canary} in {raw}"
            );
        }
        if case == 0 {
            // Positive control: every canary reached the wire.
            let requests = server.requests();
            assert_eq!(requests.len(), 1);
            let wire = format!(
                "{} {} {:?}",
                requests[0].host, requests[0].target, requests[0].headers
            );
            for canary in &all {
                assert!(
                    wire.contains(canary.as_str()),
                    "{canary} not on the wire: {wire}"
                );
            }
        }
        server.shutdown().await;
    }
}

/// An application's subscriber may select Concord's event by its own metadata — the event name,
/// the event kind, a field, for either outcome — and then receives exactly that one event.
#[tokio::test(flavor = "current_thread")]
async fn subscribers_selecting_the_event_by_name_kind_or_field_receive_it() {
    type Select = fn(&tracing::Metadata<'_>) -> bool;
    let cases: [(&str, Select, bool); 5] = [
        ("name", |m| m.name() == "concord.call", false),
        ("name, refused", |m| m.name() == "concord.call", true),
        (
            "event kind",
            |m| m.is_event() && m.target() == "concord",
            false,
        ),
        (
            "field status",
            |m| m.fields().field("status").is_some(),
            false,
        ),
        (
            "field transport",
            |m| m.fields().field("transport").is_some(),
            true,
        ),
    ];
    for (label, select, refused) in cases {
        let server = TestServer::start(|_: &Recorded| Reply::json(200, "[]")).await;
        let capture = Capture::default();
        let dispatch = capture.dispatch_selecting(select);
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let origin = if refused {
            concord_example_testserver::refused_url()
        } else {
            server.url()
        };
        let github = GitHubContext::new(
            http(),
            GitHubConfig {
                origin: concord::BaseUrl::from_static("https://api.github.com"),
                token: "t".into(),
            },
        )
        .with_base_url(origin)
        .unwrap();
        let result = github
            .repo(RepoRef::new("octo-org", "octo-repo"))
            .handle()
            .issues()
            .send()
            .await;
        assert_eq!(result.is_ok(), !refused, "{label}");
        capture.assert_call(
            "GitHub.repo.issues",
            "GET",
            "repos/{owner}/{name}/issues",
            (!refused).then_some(200),
        );
        server.shutdown().await;
    }
}

concord::api! {
    pub client SlashBase() {
        base "http://slash.invalid/api/";
        GET root();
        GET items() { path "items"; }
        GET folder() { path "items/"; }
        GET item(id: u32) { path "items/{id}"; }
    }
}

concord::api! {
    pub client BareBase() {
        base "http://bare.invalid/api";
        GET root();
    }
}

concord::api! {
    pub client HostBase() {
        base "http://host.invalid";
        GET root();
        GET items() { path "items"; }
    }
}

/// The event's route and the path actually sent for one prepared request.
async fn route_and_wire<T: 'static>(
    server: &TestServer,
    prepared: concord::Prepared<T>,
) -> (String, String) {
    let capture = Capture::default();
    let dispatch = capture.dispatch();
    let _subscriber = tracing::dispatcher::set_default(&dispatch);
    let before = server.requests().len();
    let _ = prepared.receive().await.unwrap();
    let route = match capture.concord()[0].fields.get("route") {
        Some(log_capture::Value::Text(route)) => route.clone(),
        other => panic!("route field: {other:?}"),
    };
    let target = server.requests()[before].target.clone();
    let path = target.split('?').next().unwrap();
    (route, path.strip_prefix('/').unwrap_or(path).to_owned())
}

/// The route has the shape of the sent path: a base path's trailing slash is kept when no segment
/// follows it, dropped once before appended segments; an endpoint's own trailing slash is kept.
/// Holes stay `{name}`; substituting the argument gives the wire path.
#[tokio::test(flavor = "current_thread")]
async fn the_route_has_the_shape_of_the_sent_path() {
    let server = TestServer::start(|_: &Recorded| Reply::new(204)).await;
    let slash = SlashBase::new(http())
        .with_base_url(format!("{}/api/", server.url()))
        .unwrap();
    let bare = BareBase::new(http())
        .with_base_url(format!("{}/api", server.url()))
        .unwrap();
    let host = HostBase::new(http()).with_base_url(server.url()).unwrap();
    let cases = [
        (
            route_and_wire(&server, slash.root().prepare().unwrap()).await,
            "api/",
            "",
        ),
        (
            route_and_wire(&server, slash.items().prepare().unwrap()).await,
            "api/items",
            "",
        ),
        (
            route_and_wire(&server, slash.folder().prepare().unwrap()).await,
            "api/items/",
            "",
        ),
        (
            route_and_wire(&server, slash.item(7).prepare().unwrap()).await,
            "api/items/{id}",
            "7",
        ),
        (
            route_and_wire(&server, bare.root().prepare().unwrap()).await,
            "api",
            "",
        ),
        (
            route_and_wire(&server, host.root().prepare().unwrap()).await,
            "",
            "",
        ),
        (
            route_and_wire(&server, host.items().prepare().unwrap()).await,
            "items",
            "",
        ),
    ];
    for ((route, wire), expected, id) in cases {
        assert_eq!(route, expected);
        assert_eq!(
            route.replace("{id}", id),
            wire,
            "route {route:?} vs wire {wire:?}"
        );
    }
    server.shutdown().await;
}

concord::api! {
    pub client IdentityProbe(origin: concord::BaseUrl) {
        base origin;
        scope records {
            path "records";
            GET list(id: String) { path "{id}"; page next link; }
            HEAD head();
            OPTIONS options();
            #[read]
            POST search();
            POST create();
            #[read]
            scope report {
                POST run();
            }
        }
    }
}

/// Runtime lookup and read classification agree before construction, on the prepared decoder,
/// and after execution; the event uses that identity even for a POST declared as a read.
#[tokio::test(flavor = "current_thread")]
async fn static_event_identity_agrees_with_runtime_lookup_and_read_facts() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let api = IdentityProbe::new(
        http(),
        IdentityProbeConfig {
            origin: concord::BaseUrl::parse(&server.url()).unwrap(),
        },
    );
    let records = api.records();
    let cases = [
        (
            records.list("CANARYIDENTITYPATH").prepare().unwrap(),
            __IdentityProbeOperation::RecordsList,
            "GET",
            "records/{id}",
            true,
        ),
        (
            records.head().prepare().unwrap(),
            __IdentityProbeOperation::RecordsHead,
            "HEAD",
            "records",
            true,
        ),
        (
            records.options().prepare().unwrap(),
            __IdentityProbeOperation::RecordsOptions,
            "OPTIONS",
            "records",
            true,
        ),
        (
            records.search().prepare().unwrap(),
            __IdentityProbeOperation::RecordsSearch,
            "POST",
            "records",
            true,
        ),
        (
            records.create().prepare().unwrap(),
            __IdentityProbeOperation::RecordsCreate,
            "POST",
            "records",
            false,
        ),
        (
            records.report().run().prepare().unwrap(),
            __IdentityProbeOperation::RecordsReportRun,
            "POST",
            "records",
            true,
        ),
    ];
    assert_eq!(cases.len(), __IdentityProbeOperation::ALL.len());
    for (prepared, variant, method, route, read) in cases {
        let operation = prepared.operation().unwrap();
        assert_eq!(
            __IdentityProbeOperation::from_operation(operation),
            Some(variant)
        );
        assert_eq!(operation, variant.operation());
        assert_eq!(operation.is_read(), read);
        assert_eq!(variant.operation().is_read(), read);
        assert_eq!(operation.method(), Some(method));
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let received = prepared.receive().await.unwrap();
        assert_eq!(received.operation(), Some(operation));
        assert_eq!(received.operation().unwrap().is_read(), read);
        received.decode().await.unwrap();
        capture.assert_call(operation.name(), method, route, Some(204));
        capture.assert_absent(&["CANARYIDENTITYPATH", &server.url()]);
    }
    assert_eq!(server.requests().len(), 6);
    assert!(server.requests()[0].target.contains("CANARYIDENTITYPATH"));
    server.shutdown().await;
}

/// A Followed request keeps the authored operation/template on each page, including send's
/// generated continuation path and Endpoint::prepare's path. The provider's URL never prints.
#[tokio::test(flavor = "current_thread")]
async fn followed_requests_emit_once_per_request_for_the_original_operation() {
    use concord::{Endpoint, Follow, NextUrl};
    let server = TestServer::start(|_| Reply::new(204)).await;
    let api = IdentityProbe::new(
        http(),
        IdentityProbeConfig {
            origin: concord::BaseUrl::parse(&server.url()).unwrap(),
        },
    );
    let target = format!(
        "{}/CANARYFOLLOWPATH?CANARYFOLLOWKEY=CANARYFOLLOWVALUE",
        server.url()
    );
    for page in 0..3 {
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        let endpoint = api.records().list("CANARYINITIALPATH");
        match page {
            0 => endpoint.send().await.unwrap(),
            1 => endpoint
                .follow(NextUrl::parse(&target).unwrap())
                .send()
                .await
                .unwrap(),
            _ => {
                let followed = endpoint.follow(NextUrl::parse(&target).unwrap());
                let prepared = followed.prepare().unwrap();
                let operation = prepared.operation().unwrap();
                assert_eq!(
                    __IdentityProbeOperation::from_operation(operation),
                    Some(__IdentityProbeOperation::RecordsList)
                );
                assert!(operation.is_read());
                assert_eq!(
                    prepared.summary().to_string(),
                    "IdentityProbe.records.list: GET <withheld>"
                );
                prepared.receive().await.unwrap().decode().await.unwrap();
            }
        }
        capture.assert_call(
            "IdentityProbe.records.list",
            "GET",
            "records/{id}",
            Some(204),
        );
        capture.assert_absent(&[
            "CANARYINITIALPATH",
            "CANARYFOLLOWPATH",
            "CANARYFOLLOWKEY",
            "CANARYFOLLOWVALUE",
            &server.url(),
        ]);
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].target, "/records/CANARYINITIALPATH");
    for request in &requests[1..] {
        assert_eq!(
            request.target,
            "/CANARYFOLLOWPATH?CANARYFOLLOWKEY=CANARYFOLLOWVALUE"
        );
    }
    server.shutdown().await;
}

/// A public, unmarked base is still runtime data: both its host and path are excluded from the
/// static event. Repeat with different bases so the first construction cannot become metadata.
#[tokio::test(flavor = "current_thread")]
async fn value_base_events_contain_only_authored_segments() {
    let server = TestServer::start(|_| Reply::new(204)).await;
    let host = "canaryvaluebasehost.invalid";
    let http = reqwest::Client::builder()
        .no_proxy()
        .resolve(host, server.addr())
        .build()
        .unwrap();
    for base_path in ["CANARYVALUEBASEONE", "CANARYVALUEBASETWO"] {
        let base = format!("http://{host}:{}/{base_path}/", server.addr().port());
        let api = IdentityProbe::new(
            http.clone(),
            IdentityProbeConfig {
                origin: concord::BaseUrl::parse(&base).unwrap(),
            },
        );
        let capture = Capture::default();
        let dispatch = capture.dispatch();
        let _subscriber = tracing::dispatcher::set_default(&dispatch);
        api.records()
            .list("CANARYVALUEBASEHOLE")
            .send()
            .await
            .unwrap();
        capture.assert_call(
            "IdentityProbe.records.list",
            "GET",
            "records/{id}",
            Some(204),
        );
        capture.assert_absent(&[host, base_path, &base, "CANARYVALUEBASEHOLE"]);
    }
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    for (request, base_path) in requests
        .iter()
        .zip(["CANARYVALUEBASEONE", "CANARYVALUEBASETWO"])
    {
        assert!(request.host.contains(host));
        assert_eq!(
            request.target,
            format!("/{base_path}/records/CANARYVALUEBASEHOLE")
        );
    }
    server.shutdown().await;
}

concord::api! {
    pub client DocumentProbe {
        base "http://document.invalid";
        error serde_json::Value in concord::Json;
        GET read(id: String) { path "documents/{id}"; }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn error_document_summaries_never_format_the_provider_document() {
    let document = r#"{"CANARYDOCUMENTKEY":"CANARYDOCUMENTVALUE"}"#;
    let server = TestServer::start(move |_| {
        Reply::json(403, document).header("x-private", "CANARYDOCUMENTHEADER")
    })
    .await;
    let api = DocumentProbe::new(http())
        .with_base_url(server.url())
        .unwrap();
    let capture = Capture::default();
    let dispatch = capture.dispatch();
    let _subscriber = tracing::dispatcher::set_default(&dispatch);
    let error: concord::Error<serde_json::Value> =
        api.read("CANARYDOCUMENTPATH").send().await.unwrap_err();
    // Positive control: a decoded, inspectable provider document really exists.
    assert_eq!(
        error.document().unwrap()["CANARYDOCUMENTKEY"],
        "CANARYDOCUMENTVALUE"
    );
    assert_eq!(error.provider().unwrap().body(), document.as_bytes());
    assert_eq!(
        format!("{}", error.summary()),
        "DocumentProbe.read: GET <withheld> -> 403 (status)"
    );
    assert_eq!(
        format!("{:?}", error.summary()),
        "DocumentProbe.read: GET <withheld> -> 403 (status)"
    );
    capture.assert_call("DocumentProbe.read", "GET", "documents/{id}", Some(403));
    capture.assert_absent(&[
        "CANARYDOCUMENTKEY",
        "CANARYDOCUMENTVALUE",
        "CANARYDOCUMENTPATH",
        "CANARYDOCUMENTHEADER",
    ]);
    assert_eq!(server.requests()[0].target, "/documents/CANARYDOCUMENTPATH");
    server.shutdown().await;
}
