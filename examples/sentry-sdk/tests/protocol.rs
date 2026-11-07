//! Synthetic, independent wire and failure expectations; derivations in PROVENANCE.md.
mod support;

use concord_sdk_sentry::{
    BODY_LIMIT, ERROR_BODY_LIMIT, Error, HttpClass, SendPolicy, Sentry, SentryConfig, Settings,
    concord::{ErrorKind, bytes::Bytes, reqwest::header::HeaderValue},
    http_client,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{num::NonZeroUsize, time::Duration};

const ISSUE: &str = include_str!("../fixtures/issue.json");
const ISSUES: &str = include_str!("../fixtures/issues-1.json");
const EVENT: &str = include_str!("../fixtures/event.json");
const EVENTS: &str = include_str!("../fixtures/events-1.json");
const PROBLEM: &str = include_str!("../fixtures/problem.json");

fn assert_redacted(shown: &str) {
    for secret in [
        "synthetic-bearer-secret",
        "synthetic-provider-secret",
        "synthetic-cursor-secret",
        "synthetic-query-secret",
        "fixture-only-secret",
    ] {
        assert!(!shown.contains(secret), "secret printed: {shown}");
    }
}

#[tokio::test]
async fn four_wire_shapes_encode_segments_and_keep_trailing_slashes() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert_eq!(
            r.header("authorization"),
            Some("Bearer synthetic-bearer-secret")
        );
        assert_eq!(r.header("accept"), Some("application/json"));
        assert_eq!(r.header("content-type"), None);
        assert!(r.body.is_empty());
        match r.path() {
            "/api/0/organizations/acme%2Fa%20b/issues/" => Reply::json(200, ISSUES),
            "/api/0/organizations/acme%2Fa%20b/issues/202%2Fx/" => Reply::json(200, ISSUE),
            "/api/0/organizations/acme%2Fa%20b/issues/202%2Fx/events/" => Reply::json(200, EVENTS),
            "/api/0/organizations/acme%2Fa%20b/issues/202%2Fx/events/ev%2F1/" => {
                Reply::json(200, EVENT)
            }
            other => panic!("unexpected path {other}"),
        }
    })
    .await;
    let api = support::api(&server, support::settings());
    Sentry::validate_header_names().unwrap();
    let org = api.organization("acme/a b");
    org.issues()
        .set_limit(2)
        .set_query("")
        .set_cursor("0:2:0")
        .send()
        .await
        .unwrap();
    let issue = org.issue("202/x");
    issue.get().send().await.unwrap();
    issue
        .events()
        .set_per_page(3)
        .set_query("release:one two")
        .set_cursor("0:3:0")
        .send()
        .await
        .unwrap();
    issue.event("ev/1").send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        requests[0].target,
        "/api/0/organizations/acme%2Fa%20b/issues/?limit=2&cursor=0%3A2%3A0&query="
    );
    assert_eq!(requests[1].query(), None);
    assert_eq!(
        requests[2].query(),
        Some("per_page=3&cursor=0%3A3%3A0&query=release%3Aone+two")
    );
    assert_eq!(requests[3].query(), None);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn default_queries_and_retained_base_prefix() {
    let server = TestServer::start(|_: &Recorded| Reply::json(200, "[]")).await;
    let api = support::api(&server, support::settings())
        .with_base_url(format!("{}/proxy", server.url()))
        .unwrap();
    api.organization("acme").issues().send().await.unwrap();
    api.organization("acme")
        .issue("202")
        .events()
        .send()
        .await
        .unwrap();
    assert_eq!(
        server.requests()[0].target,
        "/proxy/api/0/organizations/acme/issues/?limit=100"
    );
    assert_eq!(
        server.requests()[1].target,
        "/proxy/api/0/organizations/acme/issues/202/events/?per_page=100"
    );
}

#[tokio::test]
async fn credentials_build_errors_and_debug_do_not_print_secrets() {
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(401, PROBLEM).header("set-cookie", "synthetic-provider-secret")
    })
    .await;
    let policy = SendPolicy::new(support::settings()).unwrap();
    for token in ["", "has space", "synthetic-bearer-secret\n", "é"] {
        let error = Sentry::with_token(http_client().unwrap(), token, policy.clone()).unwrap_err();
        assert!(matches!(error, Error::Credential));
        assert_redacted(&format!("{error:?} {error}"));
    }
    let mut authorization = HeaderValue::from_static("Bearer synthetic-bearer-secret");
    authorization.set_sensitive(true);
    let config = SentryConfig {
        authorization,
        policy,
    };
    assert_redacted(&format!("{config:?}"));
    let api = Sentry::new(http_client().unwrap(), config)
        .with_base_url(server.url())
        .unwrap();
    let org = api.organization("acme");
    let op = org
        .issues()
        .set_query("synthetic-query-secret")
        .set_cursor("synthetic-cursor-secret");
    let prepared = op.clone().prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    assert_redacted(&format!("{api:?} {org:?} {op:?} {prepared:?}"));
    let error = api.organization("..").issues().send().await.unwrap_err();
    let Error::Concord(build) = &error else {
        panic!("{error:?}")
    };
    assert_eq!(build.kind(), ErrorKind::Build);
    assert!(build.head().is_none());
    assert!(server.requests().is_empty());
    let error = op.send().await.unwrap_err();
    assert_redacted(&format!("{error:?} {error}"));
    let Error::Provider(provider) = error else {
        panic!("provider error required")
    };
    assert_eq!(
        provider.document.as_ref().unwrap().detail,
        "synthetic-provider-secret"
    );
    assert_redacted(&format!("{:?}", provider.document.as_ref().unwrap()));
}

#[tokio::test]
async fn every_documented_http_class_preserves_status_and_typed_problem() {
    for (status, class) in [
        (400, HttpClass::BadRequest),
        (401, HttpClass::Unauthorized),
        (403, HttpClass::Forbidden),
        (404, HttpClass::NotFound),
        (429, HttpClass::RateLimited),
        (500, HttpClass::Server),
        (503, HttpClass::Server),
        (418, HttpClass::Other),
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(status, PROBLEM)).await;
        let error = support::api(&server, support::settings())
            .organization("acme")
            .issues()
            .send()
            .await
            .unwrap_err();
        assert_redacted(&format!("{error:?} {error}"));
        let Error::Provider(provider) = error else {
            panic!("{error:?}")
        };
        assert_eq!(provider.class, class);
        assert_eq!(provider.meta.status().as_u16(), status);
        assert_eq!(
            provider.meta.operation().unwrap().name(),
            "Sentry.organization.issues"
        );
        assert_eq!(
            provider.document.unwrap().detail,
            "synthetic-provider-secret"
        );
        assert_eq!(provider.retry.retry.attempts, 1);
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn malformed_empty_truncated_wrong_shape_and_non_utf8_success_bodies() {
    for body in [
        include_bytes!("../fixtures/malformed.json").as_slice(),
        include_bytes!("../fixtures/truncated.json").as_slice(),
        include_bytes!("../fixtures/wrong-shape.json").as_slice(),
        b"",
        b"\xff",
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(200, body)).await;
        let error = support::api(&server, support::settings())
            .organization("acme")
            .issue("202")
            .get()
            .send()
            .await
            .unwrap_err();
        assert_redacted(&format!("{error:?} {error}"));
        let Error::Concord(error) = error else {
            panic!("{error:?}")
        };
        assert_eq!(error.kind(), ErrorKind::Decode);
        assert_eq!(error.head().unwrap().status().as_u16(), 200);
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn error_document_failures_keep_authoritative_http_failure() {
    for body in [
        include_bytes!("../fixtures/malformed.json").as_slice(),
        include_bytes!("../fixtures/truncated.json").as_slice(),
        include_bytes!("../fixtures/wrong-shape.json").as_slice(),
        b"",
        b"\xff",
    ] {
        let server = TestServer::start(move |_: &Recorded| Reply::json(403, body)).await;
        let error = support::api(&server, support::settings())
            .organization("acme")
            .issue("202")
            .event("latest")
            .send()
            .await
            .unwrap_err();
        assert_redacted(&format!("{error:?} {error}"));
        let Error::Provider(provider) = error else {
            panic!("{error:?}")
        };
        assert_eq!(provider.meta.status().as_u16(), 403);
        assert_eq!(provider.document.unwrap_err().kind(), ErrorKind::Decode);
    }
}

#[tokio::test]
async fn wrong_missing_repeated_and_wrong_charset_media_are_refused() {
    for status in [200, 401] {
        for media in [
            None,
            Some("text/html"),
            Some("application/json; charset=iso-8859-1"),
            Some("repeated"),
        ] {
            let server = TestServer::start(move |_: &Recorded| {
                let reply = Reply::new(status).body(if status == 200 { EVENT } else { PROBLEM });
                match media {
                    None => reply,
                    Some("repeated") => reply
                        .header("content-type", "application/json")
                        .header("content-type", "application/json"),
                    Some(m) => reply.header("content-type", m),
                }
            })
            .await;
            let error = support::api(&server, support::settings())
                .organization("acme")
                .issue("202")
                .event("latest")
                .send()
                .await
                .unwrap_err();
            let document_error = match error {
                Error::Concord(error) if status == 200 => error,
                Error::Provider(provider) if status == 401 => {
                    assert_eq!(provider.meta.status().as_u16(), 401);
                    Box::new(provider.document.unwrap_err())
                }
                error => panic!("{error:?}"),
            };
            assert_eq!(document_error.kind(), ErrorKind::Metadata);
            assert_eq!(document_error.head().unwrap().status().as_u16(), status);
        }
    }
}

#[tokio::test]
async fn finite_and_chunked_oversized_bodies_are_bounded_on_both_status_paths() {
    for status in [200, 404] {
        for chunked in [false, true] {
            let limit = if status == 200 {
                BODY_LIMIT
            } else {
                ERROR_BODY_LIMIT
            };
            let body = Bytes::from(vec![b' '; limit + 1]);
            let server = TestServer::start(move |_: &Recorded| {
                let reply = Reply::json(status, "");
                if chunked {
                    reply.with_body(Body::Chunks(vec![(Duration::ZERO, body.clone())]))
                } else {
                    reply.body(&body)
                }
            })
            .await;
            let error = support::api(&server, support::settings())
                .organization("acme")
                .issues()
                .send()
                .await
                .unwrap_err();
            let body_error = match error {
                Error::Concord(error) if status == 200 => error,
                Error::Provider(provider) if status == 404 => {
                    assert_eq!(provider.meta.status().as_u16(), status);
                    Box::new(provider.document.unwrap_err())
                }
                error => panic!("{error:?}"),
            };
            assert_eq!(body_error.kind(), ErrorKind::Limit);
            assert_eq!(body_error.head().unwrap().status().as_u16(), status);
        }
    }
}

#[tokio::test]
async fn mid_body_disconnect_is_never_replayed_and_headless_transport_is_distinct() {
    for status in [200, 401] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::json(status, "").with_body(Body::FailAfter(vec![Bytes::from_static(
                b"{\"detail\":\"synthetic-provider-secret\"",
            )]))
        })
        .await;
        let settings = Settings {
            max_attempts: NonZeroUsize::new(3).unwrap(),
            ..support::settings()
        };
        let error = support::api(&server, settings)
            .organization("acme")
            .issue("202")
            .get()
            .send()
            .await
            .unwrap_err();
        assert_redacted(&format!("{error:?} {error}"));
        let body_error = match error {
            Error::Concord(error) if status == 200 => error,
            Error::Provider(provider) if status == 401 => Box::new(provider.document.unwrap_err()),
            error => panic!("{error:?}"),
        };
        assert_eq!(body_error.kind(), ErrorKind::Transport);
        assert_eq!(body_error.head().unwrap().status().as_u16(), status);
        assert_eq!(server.requests().len(), 1);
    }
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let error = support::api(&server, support::settings())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    let Error::Concord(error) = error else {
        panic!("{error:?}")
    };
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.head().is_none());
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn redirects_are_returned_without_forwarding_bearer() {
    let destination =
        TestServer::start(|_: &Recorded| panic!("redirect must not be followed")).await;
    let location = format!("{}/secret", destination.url());
    let server = TestServer::start(move |_: &Recorded| {
        Reply::json(302, PROBLEM).header("location", &location)
    })
    .await;
    let error = support::api(&server, support::settings())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    let Error::Provider(provider) = error else {
        panic!("{error:?}")
    };
    assert_eq!(provider.class, HttpClass::Other);
    assert_eq!(provider.meta.status().as_u16(), 302);
    assert!(destination.requests().is_empty());
}

#[tokio::test]
async fn invalid_rate_and_retry_fields_stop_scheduling_without_masking_http_errors() {
    for (name, value) in [
        ("x-sentry-rate-limit-limit", "+5"),
        ("x-sentry-rate-limit-limit", "0"),
        ("x-sentry-rate-limit-concurrentlimit", "0"),
        ("x-sentry-rate-limit-remaining", "1"),
        ("x-sentry-rate-limit-limit", "4294967296"),
        ("x-sentry-rate-limit-reset", "18446744073709551616"),
        ("retry-after", "invalid"),
        ("x-sentry-rate-limit-limit", "duplicate"),
    ] {
        for status in [200, 429] {
            let server = TestServer::start(move |_: &Recorded| {
                let reply = Reply::json(status, if status == 200 { ISSUES } else { PROBLEM });
                if value == "duplicate" {
                    reply.header(name, "5").header(name, "5")
                } else {
                    reply.header(name, value)
                }
            })
            .await;
            let settings = Settings {
                max_attempts: NonZeroUsize::new(3).unwrap(),
                ..support::settings()
            };
            let error = support::api(&server, settings)
                .organization("acme")
                .issues()
                .send()
                .await
                .unwrap_err();
            match error {
                Error::RateMetadata { meta, .. } if status == 200 => {
                    assert_eq!(meta.status().as_u16(), 200)
                }
                Error::Provider(provider) if status == 429 => {
                    assert_eq!(provider.class, HttpClass::RateLimited);
                    assert!(provider.scheduling_error.is_some());
                    assert_eq!(provider.retry.retry.attempts, 1);
                }
                error => panic!("{name}: {error:?}"),
            }
            assert_eq!(server.requests().len(), 1);
        }
    }
}

#[tokio::test]
async fn malformed_ambiguous_untrusted_or_stalled_next_links_are_errors() {
    for link in [
        "not-a-link",
        "<?cursor=x>; rel=next",
        "<?cursor=x>; rel=next; results=maybe",
        "<?cursor=x>; rel=next; results=true, <?cursor=y>; rel=next; results=true",
        "<https://other.test/?cursor=x>; rel=next; results=true",
        "</different/?cursor=x>; rel=next; results=true",
        "<?cursor=x&query=changed>; rel=next; results=true",
        "<?cursor=x&cursor=y>; rel=next; results=true",
        "<?>; rel=next; results=true",
        "<?cursor=x>; rel=next; results=true; cursor=y",
        "<?cursor=x>; rel=next; results=true; results=false",
        "<?cursor=x>; rel=next; rel=previous; results=true",
        "<?cursor=x>; rel=next; results=true; anchor=elsewhere",
        "<?cursor=current>; rel=next; results=true",
    ] {
        let server =
            TestServer::start(move |_: &Recorded| Reply::json(200, EVENTS).header("link", link))
                .await;
        let error = support::api(&server, support::settings())
            .organization("acme")
            .issue("202")
            .events()
            .set_cursor("current")
            .send()
            .await
            .unwrap_err();
        assert!(
            matches!(error, Error::Pagination { .. }),
            "{link}: {error:?}"
        );
        assert_eq!(server.requests().len(), 1);
    }
}

#[tokio::test]
async fn successful_models_responses_and_raw_link_error_formatting_are_redacted() {
    let server = TestServer::start(|r: &Recorded| {
        if r.path().ends_with("events/latest/") {
            Reply::json(200, EVENT)
        } else {
            Reply::json(200, ISSUE)
        }
    })
    .await;
    let issue = support::api(&server, support::settings())
        .organization("acme")
        .issue("202");
    let response = issue.event("latest").send().await.unwrap();
    assert_redacted(&format!("{response:?} {:?}", response.value));
    assert!(response.value.entries[0]["data"].is_object());
    let response = issue.get().send().await.unwrap();
    assert_redacted(&format!("{response:?} {:?}", response.value));
    let server = TestServer::start(|_: &Recorded| {
        Reply::json(200, ISSUES).header(
            "link",
            "<https://synthetic-provider-secret.test/?cursor=x>; rel=next; results=true",
        )
    })
    .await;
    let error = support::api(&server, support::settings())
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    assert_redacted(&format!("{error:?} {error}"));
}

#[tokio::test]
async fn invalid_local_settings_and_admission_overflow_send_nothing() {
    for settings in [
        Settings {
            timeout: Duration::ZERO,
            ..support::settings()
        },
        Settings {
            window: Duration::ZERO,
            ..support::settings()
        },
    ] {
        assert!(matches!(
            SendPolicy::new(settings),
            Err(Error::Configuration(_))
        ));
    }
    let server =
        TestServer::start(|_: &Recorded| panic!("overflow must refuse before dispatch")).await;
    let settings = Settings {
        window: Duration::MAX,
        ..support::settings()
    };
    let error = support::api(&server, settings)
        .organization("acme")
        .issues()
        .send()
        .await
        .unwrap_err();
    assert!(matches!(error, Error::Admission(_)), "{error:?}");
    assert_redacted(&format!("{error:?} {error}"));
    assert!(server.requests().is_empty());
}
