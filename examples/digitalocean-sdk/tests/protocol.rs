//! Independent wire expectations and synthetic failure responses; no live provider traffic.
mod support;

use concord_sdk_digitalocean::{
    DigitalOcean, DigitalOceanConfig, ERROR_BODY_LIMIT, Error, HttpClass, PageNumber, PageSize,
    SUCCESS_BODY_LIMIT, TagName,
    concord::{self, ErrorKind, reqwest},
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use support::*;

#[tokio::test]
async fn all_four_request_shapes_and_generated_send_policy() {
    let server = TestServer::start(workflow_reply).await;
    let api = client(&server);
    DigitalOcean::validate_header_names().unwrap();
    let list = api
        .droplets()
        .list()
        .set_per_page(size())
        .send()
        .await
        .unwrap();
    assert_eq!(list.value.droplets[0].id, 3164444);
    assert_eq!(
        list.meta.operation().unwrap().name(),
        "DigitalOcean.droplets.list"
    );
    assert_eq!(list.meta.limit().unwrap(), Some(5000));
    assert_eq!(list.meta.reset_epoch_seconds().unwrap(), Some(1402425459));
    api.droplet(3164444).get().send().await.unwrap();
    api.droplet(3164444)
        .actions()
        .set_per_page(size())
        .send()
        .await
        .unwrap();
    api.droplet(3164444).action(7515).send().await.unwrap();
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [
            "/v2/droplets?per_page=1",
            "/v2/droplets/3164444",
            "/v2/droplets/3164444/actions?per_page=1",
            "/v2/droplets/3164444/actions/7515",
        ]
    );
    assert_wire(&requests);
    server.shutdown().await;
}

#[tokio::test]
async fn optional_query_parameters_are_absent_unless_selected() {
    let server = TestServer::start(|r: &Recorded| {
        Reply::json(
            200,
            if r.path().ends_with("actions") {
                "{\"actions\":[]}"
            } else {
                "{\"droplets\":[]}"
            },
        )
    })
    .await;
    let api = client(&server);
    api.droplets().list().send().await.unwrap();
    api.droplet(42).actions().send().await.unwrap();
    api.droplets()
        .list()
        .set_page(PageNumber::new(3).unwrap())
        .set_per_page(PageSize::new(200).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.target.clone())
            .collect::<Vec<_>>(),
        [
            "/v2/droplets",
            "/v2/droplets/42/actions",
            "/v2/droplets?page=3&per_page=200",
        ]
    );
    server.shutdown().await;
}

#[test]
fn inputs_refused_before_io_and_production_prepare_only() {
    for token in ["", "contains space", "a\tb", "a\nb", "a\u{7f}", "café"] {
        assert!(matches!(
            DigitalOcean::connect(token),
            Err(Error::Credential)
        ));
    }
    for origin in [
        "https://api.digitalocean.com",
        "http://localhost:1234",
        "http://192.0.2.1",
        "http://127.0.0.1/base",
        "http://user:pass@127.0.0.1",
        "http://127.0.0.1?q=x",
        "http://127.0.0.1/#fragment",
        "SYNTHETIC_SECRET",
    ] {
        assert!(matches!(
            DigitalOcean::for_fixture(TOKEN, origin),
            Err(Error::FixtureOrigin)
        ));
    }
    assert!(PageNumber::new(0).is_err());
    assert!(PageNumber::new(1).is_ok());
    assert!(PageSize::new(0).is_err());
    assert!(PageSize::new(201).is_err());
    assert!(PageSize::new(200).is_ok());
    let api = DigitalOcean::connect(TOKEN).unwrap();
    let prepared = api.droplet(3164444).action(7515).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.digitalocean.com/v2/droplets/3164444/actions/7515"
    );
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    // Preparation only: this test does not execute the production URL.
}

#[tokio::test]
async fn every_documented_http_class_on_every_endpoint_without_retries() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/errors.json")).unwrap();
    let expected = [
        HttpClass::BadRequest,
        HttpClass::Unauthorized,
        HttpClass::Forbidden,
        HttpClass::NotFound,
        HttpClass::RateLimited,
        HttpClass::Server,
        HttpClass::Server,
        HttpClass::Other,
    ];
    for (row, expected) in rows.into_iter().zip(expected) {
        let status = row["status"].as_u64().unwrap() as u16;
        let body = row.to_string(); // Synthetic status field is an ignored provider extension.
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, &body)
                .header("retry-after", "29")
                .header("ratelimit-remaining", "0")
        })
        .await;
        let api = client(&server);
        let outcomes = [
            api.droplets().list().send().await.map(|_| ()),
            api.droplet(3164444).get().send().await.map(|_| ()),
            api.droplet(3164444).actions().send().await.map(|_| ()),
            api.droplet(3164444).action(7515).send().await.map(|_| ()),
        ];
        for outcome in outcomes {
            let Error::Provider(error) = outcome.unwrap_err() else {
                panic!("HTTP status lost")
            };
            assert_eq!(error.class(), expected);
            assert_eq!(error.meta.status().as_u16(), status);
            assert_eq!(error.meta.retry_after_seconds().unwrap(), Some(29));
            assert_eq!(error.meta.remaining().unwrap(), Some(0));
            assert!(error.document.is_ok());
        }
        assert_eq!(server.requests().len(), 4); // 401/429/5xx do not cause hidden replays.
        assert_wire(&server.requests());
        server.shutdown().await;
    }
}

// Each response is authored independently of the SDK and labeled synthetic in PROVENANCE.
fn bad_reply(case: &str, status: u16) -> Reply {
    match case {
        "malformed" => Reply::json(status, include_str!("../fixtures/malformed.json")),
        "truncated-json" => Reply::json(status, include_str!("../fixtures/truncated.json")),
        "wrong-shape" => Reply::json(status, include_str!("../fixtures/wrong-shape.json")),
        "empty" => Reply::json(status, ""),
        "wrong-media" => Reply::text(
            status,
            "text/html",
            include_str!("../fixtures/error-secret.json"),
        ),
        "missing-media" => Reply::new(status).body(include_str!("../fixtures/error-secret.json")),
        "duplicate-media" => Reply::json(status, "{}").header("content-type", "application/json"),
        "invalid-utf8" => Reply::json(status, [0xff, 0xfe]),
        "interrupted" => Reply::json(status, "").with_body(Body::FailAfter(vec![
            concord::bytes::Bytes::from_static(include_bytes!("../fixtures/truncated.json")),
        ])),
        "oversized" => Reply::json(
            status,
            vec![
                b' ';
                if status == 200 {
                    SUCCESS_BODY_LIMIT
                } else {
                    ERROR_BODY_LIMIT
                } + 1
            ],
        ),
        _ => panic!("unknown synthetic case"),
    }
}

#[tokio::test]
async fn success_body_failure_stages_keep_status_operation_and_redaction() {
    for (case, kind) in [
        ("malformed", ErrorKind::Decode),
        ("truncated-json", ErrorKind::Decode),
        ("wrong-shape", ErrorKind::Decode),
        ("empty", ErrorKind::Decode),
        ("wrong-media", ErrorKind::Metadata),
        ("missing-media", ErrorKind::Metadata),
        ("duplicate-media", ErrorKind::Metadata),
        ("invalid-utf8", ErrorKind::Decode),
        ("interrupted", ErrorKind::Transport),
        ("oversized", ErrorKind::Limit),
    ] {
        let server = TestServer::start(move |_: &Recorded| bad_reply(case, 200)).await;
        let error = client(&server).droplets().list().send().await.unwrap_err();
        assert_redacted(&error);
        let Error::Concord(error) = error else {
            panic!("wrong stage for {case}")
        };
        assert_eq!(error.kind(), kind, "{case}");
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        assert_eq!(
            error.operation().unwrap().name(),
            "DigitalOcean.droplets.list"
        );
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unreadable_error_documents_preserve_authoritative_status() {
    for (case, kind) in [
        ("malformed", ErrorKind::Decode),
        ("truncated-json", ErrorKind::Decode),
        ("wrong-shape", ErrorKind::Decode),
        ("empty", ErrorKind::Decode),
        ("wrong-media", ErrorKind::Metadata),
        ("missing-media", ErrorKind::Metadata),
        ("duplicate-media", ErrorKind::Metadata),
        ("interrupted", ErrorKind::Transport),
        ("oversized", ErrorKind::Limit),
    ] {
        let server = TestServer::start(move |_: &Recorded| bad_reply(case, 429)).await;
        let error = client(&server)
            .droplet(3164444)
            .action(7515)
            .send()
            .await
            .unwrap_err();
        assert_redacted(&error);
        let Error::Provider(error) = error else {
            panic!("HTTP failure lost for {case}")
        };
        assert_eq!(error.meta.status().as_u16(), 429);
        assert_eq!(error.class(), HttpClass::RateLimited);
        assert_eq!(error.document.unwrap_err().kind(), kind);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn headless_transport_failure_has_no_response_head_and_is_not_retried() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = client(&server).droplets().list().send().await.unwrap_err();
    assert_redacted(&error);
    let Error::Concord(error) = error else {
        panic!("wrong transport stage")
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.head().is_none());
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn redirects_are_returned_and_never_contact_the_target() {
    let trap = TestServer::start(|_: &Recorded| panic!("redirect followed")).await;
    for status in [301, 302, 303, 307, 308] {
        let location = format!(
            "{}/SYNTHETIC_SECRET",
            trap.url().replace("127.0.0.1", "localhost")
        );
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, include_str!("../fixtures/error-secret.json"))
                .header("location", &location)
                .header("set-cookie", TOKEN)
        })
        .await;
        let error = client(&server).droplets().list().send().await.unwrap_err();
        assert_redacted(&error);
        let Error::Provider(error) = error else {
            panic!("redirect status lost")
        };
        assert_eq!(error.meta.status().as_u16(), status);
        assert_eq!(error.class(), HttpClass::Other);
        assert!(format!("{error:?}").contains("set-cookie"));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
    assert!(trap.requests().is_empty());
    trap.shutdown().await;
}

#[tokio::test]
async fn next_link_validation_rejects_origin_path_userinfo_and_malformed_targets() {
    for attack in [
        "port",
        "scheme",
        "path",
        "userinfo",
        "fragment",
        "relative",
        "empty",
        "invalid",
        "control",
        "backslash",
    ] {
        let server = TestServer::start(move |r: &Recorded| {
            let origin = format!("http://{}", r.host);
            let target = match attack {
                "port" => "http://127.0.0.1:1/v2/droplets".to_owned(),
                "scheme" => format!("https://{}/v2/droplets", r.host),
                "path" => format!("{origin}/v2/account?secret={TOKEN}"),
                "userinfo" => format!("http://{TOKEN}@{}/v2/droplets", r.host),
                "fragment" => format!("{origin}/v2/droplets#{TOKEN}"),
                "relative" => "/v2/droplets?page=2".into(),
                "empty" => String::new(),
                "invalid" => TOKEN.into(),
                "control" => format!("{origin}/v2/droplets\n?page=2"),
                "backslash" => format!("{origin}\\v2/droplets"),
                _ => unreachable!(),
            };
            Reply::json(
                200,
                serde_json::json!({"droplets":[],"links":{"pages":{"next":target}}}).to_string(),
            )
        })
        .await;
        let error = client(&server)
            .droplet_pages(size(), cap(2))
            .next_page()
            .await
            .unwrap_err();
        let provider = error.provider_error().unwrap();
        assert!(matches!(provider, Error::UnsafeNext), "{attack}");
        assert_redacted(provider);
        assert!(!format!("{error:?}").contains(TOKEN));
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn quota_fields_distinguish_absence_invalid_repeated_and_overflow() {
    for (value, expected) in [
        (None, Ok(None)),
        (Some("29"), Ok(Some(29))),
        (Some("+5"), Err(concord::FieldError::Invalid)),
        (
            Some("18446744073709551616"),
            Err(concord::FieldError::Overflow),
        ),
        (Some("REPEAT"), Err(concord::FieldError::Repeated)),
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            let mut reply = Reply::json(200, "{\"droplets\":[]}");
            for header in [
                "ratelimit-limit",
                "ratelimit-remaining",
                "ratelimit-reset",
                "retry-after",
            ] {
                if let Some(value) = value {
                    reply = reply.header(header, value);
                    if value == "REPEAT" {
                        reply = reply.header(header, "1");
                    }
                }
            }
            reply
        })
        .await;
        let response = client(&server).droplets().list().send().await.unwrap();
        assert_eq!(response.meta.limit(), expected);
        assert_eq!(response.meta.remaining(), expected);
        assert_eq!(response.meta.reset_epoch_seconds(), expected);
        assert_eq!(response.meta.retry_after_seconds(), expected);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn credentials_documents_headers_and_followed_urls_stay_out_of_formatting() {
    let authorization = concord_auth::bearer(TOKEN).unwrap();
    let config = DigitalOceanConfig { authorization };
    assert!(!format!("{config:?}").contains(TOKEN));
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(403, include_str!("../fixtures/error-secret.json")).header("set-cookie", TOKEN)
    })
    .await;
    let api = client(&server);
    for shown in [
        format!("{api:?}"),
        format!("{:?}", api.droplet(1)),
        format!("{:?}", api.droplet(1).action(2)),
        format!("{:?}", api.droplet(1).get().prepare().unwrap()),
    ] {
        assert!(!shown.contains(TOKEN));
    }
    let prepared = api
        .droplets()
        .list()
        .prepare()
        .unwrap()
        .follow(&format!("{}/v2/droplets?token={TOKEN}", server.url()))
        .unwrap();
    assert!(!format!("{prepared:?}").contains(TOKEN));
    let error = api.droplet(1).get().send().await.unwrap_err();
    assert_redacted(&error);
    let Error::Provider(provider) = error else {
        panic!("expected HTTP error")
    };
    assert_eq!(provider.class(), HttpClass::Forbidden);
    let document = provider.document.unwrap();
    assert_eq!(document.message, TOKEN); // Deliberate unredacted field access still works.
    assert!(!format!("{document:?}").contains(TOKEN));
    let build = DigitalOcean::connect(TOKEN)
        .unwrap()
        .with_base_url(TOKEN)
        .unwrap_err();
    assert_eq!(build.kind(), ErrorKind::Build);
    assert_redacted(&Error::from(build));
    for error in [
        Error::Credential,
        Error::FixtureOrigin,
        Error::TransportSetup,
        Error::PageNumber,
        Error::PageSize,
        Error::UnsafeNext,
    ] {
        assert_redacted(&error);
    }
    let header = reqwest::header::HeaderValue::from_static("Bearer SYNTHETIC_SECRET");
    assert!(!format!("{:?}", api.set_authorization(header)).contains(TOKEN));
    server.shutdown().await;
}

fn assert_redacted(error: &Error) {
    assert!(!format!("{error}").contains(TOKEN));
    assert!(!format!("{error:?}").contains(TOKEN));
    assert!(std::error::Error::source(error).is_none());
}

// Maintenance: tag-filtered droplet listing and droplet snapshots (see MAINTENANCE.md).

#[tokio::test]
async fn tag_filter_and_snapshot_request_shapes() {
    let server = TestServer::start(|r: &Recorded| {
        Reply::json(
            200,
            if r.path().ends_with("snapshots") {
                "{\"snapshots\":[]}"
            } else {
                "{\"droplets\":[]}"
            },
        )
    })
    .await;
    let api = client(&server);
    let tagged = api
        .droplets()
        .list()
        .set_tag_name(prod())
        .send()
        .await
        .unwrap();
    assert_eq!(
        tagged.meta.operation().unwrap().name(),
        "DigitalOcean.droplets.list"
    );
    // Reserved query characters in a tag stay inside its one value.
    let odd = TagName::new("team+ops&tag_name=x#y").unwrap();
    api.droplets()
        .list()
        .set_page(PageNumber::new(2).unwrap())
        .set_per_page(size())
        .set_tag_name(odd)
        .send()
        .await
        .unwrap();
    let snapshots = api.droplet(42).snapshots().send().await.unwrap();
    assert_eq!(
        snapshots.meta.operation().unwrap().name(),
        "DigitalOcean.droplet.snapshots"
    );
    api.droplet(42)
        .snapshots()
        .set_per_page(PageSize::new(200).unwrap())
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(
        requests
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [
            "/v2/droplets?tag_name=env%3Aprod",
            "/v2/droplets?page=2&per_page=1&tag_name=team%2Bops%26tag_name%3Dx%23y",
            "/v2/droplets/42/snapshots",
            "/v2/droplets/42/snapshots?per_page=200",
        ]
    );
    assert_eq!(requests[1].query_all("tag_name"), ["team+ops&tag_name=x#y"]);
    assert_wire(&requests);
    let prepared = DigitalOcean::connect(TOKEN)
        .unwrap()
        .droplets()
        .list()
        .set_tag_name(prod())
        .prepare()
        .unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "https://api.digitalocean.com/v2/droplets?tag_name=env%3Aprod"
    );
    // Preparation only: this test does not execute the production URL.
    server.shutdown().await;
}

#[tokio::test]
async fn every_documented_http_class_on_the_new_endpoints_without_retries() {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/errors.json")).unwrap();
    for row in rows {
        let status = row["status"].as_u64().unwrap() as u16;
        let body = row.to_string();
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, &body).header("retry-after", "29")
        })
        .await;
        let api = client(&server);
        let outcomes = [
            api.droplets()
                .list()
                .set_tag_name(prod())
                .send()
                .await
                .map(|_| ()),
            api.droplet(3164444).snapshots().send().await.map(|_| ()),
        ];
        for outcome in outcomes {
            let error = outcome.unwrap_err();
            assert_redacted(&error);
            let Error::Provider(error) = error else {
                panic!("HTTP status lost")
            };
            assert_eq!(error.meta.status().as_u16(), status);
            assert_eq!(error.meta.retry_after_seconds().unwrap(), Some(29));
            assert!(error.document.is_ok());
        }
        let first = client(&server)
            .tagged_droplet_pages(&prod(), size(), cap(2))
            .next_page()
            .await
            .unwrap_err();
        assert!(matches!(first.provider_error(), Some(Error::Provider(_))));
        assert_eq!(server.requests().len(), 3);
        assert_wire(&server.requests());
        server.shutdown().await;
    }
}

#[tokio::test]
async fn snapshot_body_failures_keep_status_and_operation() {
    for (case, kind) in [
        ("malformed", ErrorKind::Decode),
        ("wrong-shape", ErrorKind::Decode),
        ("wrong-media", ErrorKind::Metadata),
        ("oversized", ErrorKind::Limit),
    ] {
        let server = TestServer::start(move |_: &Recorded| match case {
            "wrong-shape" => Reply::json(200, "{\"snapshots\":[{\"id\":1,\"name\":\"x\"}]}"),
            _ => bad_reply(case, 200),
        })
        .await;
        let error = client(&server)
            .droplet(3164444)
            .snapshots()
            .send()
            .await
            .unwrap_err();
        assert_redacted(&error);
        let Error::Concord(error) = error else {
            panic!("wrong stage for {case}")
        };
        assert_eq!(error.kind(), kind, "{case}");
        assert_eq!(
            error.operation().unwrap().name(),
            "DigitalOcean.droplet.snapshots"
        );
        server.shutdown().await;
    }
    assert_redacted(&Error::TagName);
}
