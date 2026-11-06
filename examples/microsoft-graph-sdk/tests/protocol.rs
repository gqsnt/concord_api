//! Wire-level tests against synthetic, hand-authored Graph fixtures only.
mod support;

use std::num::NonZeroUsize;

use concord::{ErrorKind, bytes::Bytes};
use concord_example_testserver::{Body, Reply, TestServer};
use concord_sdk_microsoft_graph::{
    AuthContext, ConfigurationProblem, ErrorBody, ErrorClass, Field, GraphClient, GraphError,
    GroupField, LinkProblem, MemberField, ProtocolProblem, UserField,
};
use serde_json::json;
use support::*;

#[tokio::test]
async fn user_delta_get_select_auth_accept_and_empty_body() {
    let server = TestServer::start(|r| reply(r, INITIAL)).await;
    let client = client(&server, "tenant-a");
    let page = client
        .users_delta(&[UserField::Mail, UserField::DisplayName, UserField::Mail])
        .unwrap()
        .fetch(None)
        .await
        .unwrap();
    assert_get(
        &server.single(),
        "/v1.0/users/delta?%24select=displayName%2Cid%2Cmail",
        "tenant-a",
    );
    assert_eq!(
        page.items[0].display_name,
        Field::Value("Ada Example".into())
    );
    assert_eq!(page.items[1].mail, Field::Null);
    assert_eq!(page.items[0].surname, Field::Missing);
    assert!(page.meta.delta_link.is_none());
    assert_eq!(
        page.meta.response.operation().unwrap().name(),
        "GraphApi.users_delta"
    );
    assert_eq!(
        page.meta.response.headers()["request-id"],
        "synthetic-success-id"
    );
    assert!(!format!("{client:?} {page:?} {:?}", page.next).contains("synthetic-token"));
    server.shutdown().await;
}

#[tokio::test]
async fn group_get_and_selected_fields() {
    let server = TestServer::start(|r| reply(r, GROUPS_FINAL)).await;
    let page = client(&server, "tenant-a")
        .groups(&[GroupField::DisplayName])
        .unwrap()
        .fetch(None)
        .await
        .unwrap();
    assert_get(
        &server.single(),
        "/v1.0/groups?%24select=displayName%2Cid",
        "tenant-a",
    );
    assert_eq!(
        page.items[0].group_types.as_deref(),
        Some(["Unified".to_owned()].as_slice())
    );
    assert!(page.next.is_none() && page.meta.delta_link.is_none());
    server.shutdown().await;
}

#[tokio::test]
async fn member_get_encodes_group_as_one_segment_and_keeps_advanced_headers() {
    let server = TestServer::start(|_| Reply::json(200, b"{\"value\":[]}")).await;
    client(&server, "tenant-a")
        .direct_members("group/one", &[MemberField::DisplayName])
        .unwrap()
        .fetch(None)
        .await
        .unwrap();
    let request = server.single();
    assert_get(
        &request,
        "/v1.0/groups/group%2Fone/members?%24select=displayName%2Cid&%24count=true",
        "tenant-a",
    );
    assert_eq!(request.header("consistencylevel"), Some("eventual"));
    server.shutdown().await;
}

#[tokio::test]
async fn opaque_next_url_preserves_query_bytes_and_does_not_repeat_select() {
    let server = TestServer::start(|r| {
        if r.query().is_some_and(|q| q.contains("skiptoken")) {
            reply(r, EMPTY)
        } else {
            reply(r, INITIAL)
        }
    })
    .await;
    let listing = client(&server, "tenant-a")
        .users_delta(&[UserField::DisplayName])
        .unwrap();
    let first = listing.fetch(None).await.unwrap();
    let next = first.next.unwrap();
    let restored = listing.restore_next(&next.to_json()).unwrap();
    let second = listing.fetch(Some(&restored)).await.unwrap();
    assert!(second.items.is_empty() && second.next.is_some());
    assert_get(
        &server.requests()[1],
        "/v1.0/users/delta?$skiptoken=synthetic%2Bpage%2F2%3D&opaque=a+b&opaque=%252F",
        "tenant-a",
    );
    assert!(!format!("{next:?}").contains("skiptoken"));
    server.shutdown().await;
}

#[tokio::test]
async fn provider_errors_preserve_code_nested_details_metadata_and_retry_after() {
    let cases: &[(u16, &[u8], ErrorClass, &str)] = &[
        (
            400,
            include_bytes!("../fixtures/error-bad-request.json"),
            ErrorClass::InvalidRequest,
            "Request_UnsupportedQuery",
        ),
        (
            401,
            include_bytes!("../fixtures/error-unauthorized.json"),
            ErrorClass::Authentication,
            "InvalidAuthenticationToken",
        ),
        (
            403,
            include_bytes!("../fixtures/error-forbidden.json"),
            ErrorClass::Permission,
            "Authorization_RequestDenied",
        ),
        (
            404,
            include_bytes!("../fixtures/error-not-found.json"),
            ErrorClass::NotFound,
            "Request_ResourceNotFound",
        ),
        (
            410,
            include_bytes!("../fixtures/error-gone.json"),
            ErrorClass::SyncStateExpired,
            "syncStateNotFound",
        ),
        (
            429,
            include_bytes!("../fixtures/error-throttled.json"),
            ErrorClass::Throttled,
            "TooManyRequests",
        ),
        (
            503,
            include_bytes!("../fixtures/error-service.json"),
            ErrorClass::Service,
            "ServiceUnavailable",
        ),
    ];
    for &(status, body, class, code) in cases {
        let body = body.to_vec();
        let server = TestServer::start(move |_| {
            Reply::json(status, &body)
                .header("request-id", "synthetic-header-id")
                .header("client-request-id", "synthetic-client-id")
                .header("retry-after", "17")
                .header("location", "https://example.invalid/no-reset")
        })
        .await;
        let error = client(&server, "tenant-a")
            .users_delta(&[])
            .unwrap()
            .fetch(None)
            .await
            .unwrap_err();
        assert_eq!(error.class(), class);
        assert_eq!(error.provider().unwrap().code, code);
        let meta = error.meta().unwrap();
        assert_eq!(meta.status().as_u16(), status);
        assert_eq!(meta.headers()["retry-after"], "17");
        assert_eq!(meta.headers()["request-id"], "synthetic-header-id");
        assert_eq!(meta.headers()["client-request-id"], "synthetic-client-id");
        assert_eq!(meta.operation().unwrap().name(), "GraphApi.users_delta");
        if status == 403 {
            let provider = error.provider().unwrap();
            assert!(provider.has_code("insufficient_claims"));
            assert_eq!(provider.details.len(), 1);
            assert_eq!(
                provider.inner_error.as_ref().unwrap().additional["request-id"],
                "synthetic-request-403"
            );
        }
        let rendered = format!("{error} {error:?}");
        assert!(!rendered.contains("Synthetic") && !rendered.contains("synthetic-header-id"));
        assert!(!rendered.contains("example.invalid") && !rendered.contains("synthetic-token"));
        assert_eq!(server.requests().len(), 1); // no retry or reset
        server.shutdown().await;
    }
}

#[tokio::test]
async fn expired_nested_state_code_is_classified_without_automatic_restart() {
    let server = TestServer::start(|_| Reply::json(400,
        br#"{"error":{"code":"BadRequest","message":"synthetic","innerError":{"code":"syncStateNotFound"}}}"#)).await;
    let error = client(&server, "tenant-a")
        .users_delta(&[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert_eq!(error.class(), ErrorClass::SyncStateExpired);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_and_oversized_error_bodies_keep_actual_http_failure() {
    let server = TestServer::start(|_| {
        Reply::text(
            503,
            "text/plain",
            include_bytes!("../fixtures/non-json-error.txt"),
        )
        .header("request-id", "synthetic-error-body")
    })
    .await;
    let api = client(&server, "tenant-a");
    let error = api.groups(&[]).unwrap().fetch(None).await.unwrap_err();
    assert!(matches!(
        error,
        GraphError::Provider {
            body: ErrorBody::Malformed,
            ..
        }
    ));
    assert_eq!(error.class(), ErrorClass::Service);
    let api = api.with_body_limits(
        NonZeroUsize::new(1024).unwrap(),
        NonZeroUsize::new(8).unwrap(),
    );
    let error = api.groups(&[]).unwrap().fetch(None).await.unwrap_err();
    assert_eq!(
        error.meta().unwrap().headers()["request-id"],
        "synthetic-error-body"
    );
    assert!(
        matches!(error, GraphError::Provider { body: ErrorBody::Unavailable(ref e), .. } if e.kind() == ErrorKind::Limit)
    );
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_success_and_media_type_failures_keep_response_head() {
    for (body, content_type, kind) in [
        (
            include_bytes!("../fixtures/truncated-json.json").as_slice(),
            "application/json",
            ErrorKind::Decode,
        ),
        (
            include_bytes!("../fixtures/missing-value.json").as_slice(),
            "application/json",
            ErrorKind::Decode,
        ),
        (
            b"{\"value\":[]}".as_slice(),
            "text/html",
            ErrorKind::Metadata,
        ),
    ] {
        let server = TestServer::start(move |_| {
            Reply::text(200, content_type, body).header("request-id", "synthetic-decode-id")
        })
        .await;
        let error = client(&server, "tenant-a")
            .groups(&[])
            .unwrap()
            .fetch(None)
            .await
            .unwrap_err();
        assert_eq!(error.meta().unwrap().status().as_u16(), 200);
        assert_eq!(
            error.meta().unwrap().headers()["request-id"],
            "synthetic-decode-id"
        );
        assert!(matches!(error, GraphError::Concord { ref error, .. } if error.kind() == kind));
        server.shutdown().await;
    }
}

#[tokio::test]
async fn success_body_limit_and_interrupted_body_are_not_complete_pages() {
    let server = TestServer::start(|r| reply(r, GROUPS_FINAL)).await;
    let api = client(&server, "tenant-a").with_body_limits(
        NonZeroUsize::new(10).unwrap(),
        NonZeroUsize::new(1024).unwrap(),
    );
    let error = api.groups(&[]).unwrap().fetch(None).await.unwrap_err();
    assert!(
        matches!(error, GraphError::Concord { ref error, .. } if error.kind() == ErrorKind::Limit)
    );
    assert_eq!(error.meta().unwrap().status().as_u16(), 200);
    server.shutdown().await;

    let server = TestServer::start(|_| {
        Reply::json(200, b"")
            .with_body(Body::FailAfter(vec![Bytes::from_static(b"{\"value\":[]}")]))
    })
    .await;
    let error = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, GraphError::Concord { ref error, .. } if error.kind() == ErrorKind::Transport)
    );
    assert_eq!(error.meta().unwrap().status().as_u16(), 200);
    server.shutdown().await;
}

#[tokio::test]
async fn conflicting_or_missing_delta_links_are_protocol_errors() {
    for fixture in [
        include_bytes!("../fixtures/delta-both-links.json").as_slice(),
        include_bytes!("../fixtures/delta-no-links.json").as_slice(),
    ] {
        let server = TestServer::start(move |r| reply(r, fixture)).await;
        let error = client(&server, "tenant-a")
            .users_delta(&[])
            .unwrap()
            .fetch(None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            GraphError::Protocol {
                problem: ProtocolProblem::DeltaLinkCardinality,
                ..
            }
        ));
        assert_eq!(error.meta().unwrap().status().as_u16(), 200);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn regular_collection_rejects_delta_links_and_incomplete_member_types() {
    let server =
        TestServer::start(|r| reply(r, include_bytes!("../fixtures/collection-with-delta.json")))
            .await;
    let error = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GraphError::Protocol {
            problem: ProtocolProblem::UnexpectedDeltaLink,
            ..
        }
    ));
    server.shutdown().await;
    let server =
        TestServer::start(|r| reply(r, include_bytes!("../fixtures/member-missing-type.json")))
            .await;
    let error = client(&server, "tenant-a")
        .direct_members("group-one", &[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert!(
        matches!(error, GraphError::Concord { ref error, .. } if error.kind() == ErrorKind::Decode)
    );
    server.shutdown().await;
}

#[tokio::test]
async fn empty_ids_and_unexpected_success_status_are_rejected() {
    for (status, body, problem) in [
        (
            200,
            b"{\"value\":[{\"id\":\"\"}]}".as_slice(),
            ProtocolProblem::InvalidObject,
        ),
        (204, b"".as_slice(), ProtocolProblem::UnexpectedStatus),
    ] {
        let server = TestServer::start(move |_| Reply::json(status, body)).await;
        let error = client(&server, "tenant-a")
            .groups(&[])
            .unwrap()
            .fetch(None)
            .await
            .unwrap_err();
        assert!(matches!(error, GraphError::Protocol { problem: p, .. } if p == problem));
        assert_eq!(error.meta().unwrap().status().as_u16(), status);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unsafe_returned_links_are_refused_before_emission_or_following() {
    let cases = [
        "https://example.invalid/v1.0/users/delta?$skiptoken=synthetic",
        "/v1.0/users/delta?$skiptoken=synthetic",
        "{{BASE}}/users/delta?$skiptoken=synthetic#fragment",
        "{{BASE}}/groups?$skiptoken=synthetic",
        "{{BASE}}/users/delta/extra?$skiptoken=synthetic",
        "{{BASE}}/users/delta?bad=synthetic\nvalue",
        "{{BASE}}/users/delta?bad=synthetic\\value",
        "http://synthetic-user@{{HOST}}/v1.0/users/delta?$skiptoken=synthetic",
        "https://{{HOST}}/v1.0/users/delta?$skiptoken=synthetic",
    ];
    for target in cases {
        let server = TestServer::start(move |r| {
            let host = r.header("host").unwrap();
            let target = target
                .replace("{{HOST}}", host)
                .replace("{{BASE}}", &format!("http://{host}/v1.0"));
            Reply::json(
                200,
                json!({"value":[{"id":"should-not-emit"}], "@odata.nextLink":target}).to_string(),
            )
        })
        .await;
        let error = client(&server, "tenant-a")
            .users_delta(&[])
            .unwrap()
            .fetch(None)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            GraphError::Link {
                problem: LinkProblem::UnsafeDestination,
                ..
            }
        ));
        assert_eq!(error.meta().unwrap().status().as_u16(), 200);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn next_link_self_loop_is_not_followed() {
    let server = TestServer::start(|r| {
        Reply::json(200, json!({
        "value":[], "@odata.nextLink":format!("http://{}{}", r.header("host").unwrap(), r.target)
    }).to_string())
    })
    .await;
    let error = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        GraphError::Link {
            problem: LinkProblem::DidNotAdvance,
            ..
        }
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}

#[tokio::test]
async fn redirect_does_not_forward_credentials_even_on_the_same_origin() {
    let sink = TestServer::start(|_| Reply::json(200, b"{\"value\":[]}")).await;
    let destination = sink.url();
    let server = TestServer::start(move |r| {
        let location = if r.query().is_some() {
            format!("http://{}/capture", r.header("host").unwrap())
        } else {
            format!("{destination}/capture")
        };
        Reply::new(302).header("location", location)
    })
    .await;
    let api = client(&server, "tenant-a");
    for fields in [&[][..], &[GroupField::DisplayName][..]] {
        let error = api.groups(fields).unwrap().fetch(None).await.unwrap_err();
        assert_eq!(error.class(), ErrorClass::OtherHttp);
        assert_eq!(error.meta().unwrap().status().as_u16(), 302);
    }
    assert_eq!(server.requests().len(), 2);
    assert!(sink.requests().is_empty());
    server.shutdown().await;
    sink.shutdown().await;
}

#[test]
fn configuration_rejects_credential_injection_and_non_loopback_destinations() {
    let context = AuthContext::new("synthetic-tenant", "synthetic-principal").unwrap();
    for token in [
        "",
        "synthetic\r\nX-Evil: yes",
        "Bearer synthetic",
        "synthetic token",
    ] {
        assert!(matches!(
            GraphClient::new(context.clone(), token),
            Err(GraphError::Configuration(
                ConfigurationProblem::InvalidToken
            ))
        ));
    }
    for base in [
        "http://example.invalid/v1.0",
        "http://localhost/v1.0",
        "https://127.0.0.1/v1.0",
        "http://127.0.0.1/v1.0?x=y",
        "http://127.0.0.1/beta",
        "http://synthetic@127.0.0.1/v1.0",
        "http://127.0.0.1:0/v1.0",
    ] {
        assert!(matches!(
            GraphClient::for_loopback(context.clone(), "synthetic-token", base),
            Err(GraphError::Configuration(
                ConfigurationProblem::InvalidLoopbackBase
            ))
        ));
    }
    assert!(AuthContext::new(" ", "synthetic-principal").is_err());
    assert!(AuthContext::new("synthetic-tenant", "").is_err());
}

#[tokio::test]
async fn transport_failure_retains_generated_identity_without_token_or_url() {
    let server = TestServer::start(|_| Reply::hang_up()).await;
    let error = client(&server, "tenant-a")
        .groups(&[])
        .unwrap()
        .fetch(None)
        .await
        .unwrap_err();
    assert!(error.meta().is_none());
    match &error {
        GraphError::Concord { error, .. } => {
            assert_eq!(error.kind(), ErrorKind::Transport);
            assert_eq!(error.endpoint(), Some("GraphApi.groups"));
        }
        _ => panic!("expected transport failure"),
    }
    assert!(!format!("{error:?} {error}").contains("synthetic-token"));
    assert!(!format!("{error:?} {error}").contains(&server.url()));
    server.shutdown().await;
}
