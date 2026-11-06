//! Offline wire contracts and negative protocol cases, through the public SDK.
mod common;
use common::*;
use concord_example_testserver::{Body, Reply as WireReply, Script, TestServer};
use concord_sdk_snowflake::*;
use std::time::Duration;

#[tokio::test]
async fn all_four_operations_have_exact_targets_headers_and_bodies() {
    let script = Script::new([
        json(202, PENDING_A),
        json(200, TWO),
        gzip(GZIP1),
        json(
            200,
            &mutate(CANCELLED, |v| {
                v["statementHandle"] = A.into();
            }),
        ),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let c = client(&s);
    let submitted = c.submit(context(), statement()).await.unwrap();
    assert_eq!(submitted.query.context(), &context());
    assert_eq!(submitted.query.handle().as_str(), A);
    let first = first(submitted.query.status().await.unwrap());
    let p = first.plan().fetch(1).await.unwrap();
    assert_eq!(p.rows[0][1].as_deref(), Some("βeta"));
    assert_eq!(p.meta.headers()["content-encoding"], "gzip");
    assert_eq!(p.meta.headers()["content-length"], GZIP1.len().to_string());
    submitted.query.cancel(id(CANCEL_ID)).await.unwrap();
    let requests = s.requests();
    assert_eq!(requests.len(), 4);
    for r in &requests {
        assert_eq!(
            r.header("authorization"),
            Some("Bearer SYNTHETIC_NOT_A_CREDENTIAL")
        );
        assert_eq!(
            r.header("x-snowflake-authorization-token-type"),
            Some("OAUTH")
        );
        assert_eq!(r.header("user-agent"), Some("concord-sdk-snowflake/0.0.0"));
        assert_eq!(r.header("accept"), Some("application/json"));
        assert_eq!(r.header("content-type"), Some("application/json"));
        assert!(!r.chunked);
    }
    assert_eq!(requests[0].method, "POST");
    assert_eq!(
        requests[0].target,
        format!("/api/v2/statements?requestId={RID}&async=true")
    );
    assert_eq!(requests[0].body, include_bytes!("../fixtures/submit.json"));
    assert_eq!(requests[1].method, "GET");
    assert_eq!(requests[1].target, format!("/api/v2/statements/{A}"));
    assert_eq!(requests[2].method, "GET");
    assert_eq!(requests[2].header("accept-encoding"), Some("gzip"));
    assert_eq!(
        requests[2].target,
        format!("/api/v2/statements/{A}?partition=1")
    );
    assert_eq!(requests[3].method, "POST");
    assert_eq!(
        requests[3].target,
        format!("/api/v2/statements/{A}/cancel?requestId={CANCEL_ID}")
    );
    for r in &requests[1..] {
        assert!(r.body.is_empty());
    }
    s.shutdown().await;
}

#[tokio::test]
async fn provider_error_keeps_code_sqlstate_headers_raw_body_and_generated_operation() {
    let body = include_bytes!("../fixtures/failed-a.json");
    let s = TestServer::start(|_| json(422, include_bytes!("../fixtures/failed-a.json"))).await;
    let e = client(&s).submit(context(), statement()).await.unwrap_err();
    let Error::Server(e) = e else {
        panic!("not server error")
    };
    assert_eq!(e.document.as_ref().unwrap().code, "000904");
    assert_eq!(
        e.document.as_ref().unwrap().sql_state.as_deref(),
        Some("42000")
    );
    assert_eq!(e.body.as_ref(), body);
    assert_eq!(e.meta.status().as_u16(), 422);
    assert_eq!(
        e.meta.headers()["x-snowflake-request-id"],
        "synthetic-response-id"
    );
    assert_eq!(e.meta.operation().unwrap().name(), "SqlApi.submit");
    assert!(e.decode_error.is_none());
    s.shutdown().await;
}

#[tokio::test]
async fn malformed_and_non_json_server_errors_keep_status_and_evidence() {
    for (status, body) in [
        (502, include_bytes!("../fixtures/proxy.html").as_slice()),
        (429, include_bytes!("../fixtures/malformed.json").as_slice()),
    ] {
        let bytes = body.to_vec();
        let s = TestServer::start(move |_| {
            WireReply::text(status, "text/html", &bytes).header("retry-after", "7")
        })
        .await;
        let Error::Server(e) = client(&s).submit(context(), statement()).await.unwrap_err() else {
            panic!("not server error")
        };
        assert_eq!(e.meta.status().as_u16(), status);
        assert_eq!(e.meta.headers()["retry-after"], "7");
        assert!(e.decode_error.is_some());
        assert_eq!(e.body.as_ref(), body);
        s.shutdown().await;
    }
}

#[tokio::test]
async fn body_limits_apply_after_gzip_and_keep_error_heads() {
    let script = Script::new([json(200, TWO), gzip(GZIP1)]);
    let s = TestServer::start(move |_| script.next()).await;
    let first = first(
        client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .state,
    );
    // A separate low limit client exercises successful-body limiting without invalid discovery.
    let huge = vec![b' '; 2000];
    let overflow = TestServer::start(move |_| json(200, &huge)).await;
    let c = Client::new(
        &overflow.url(),
        "SYNTHETIC",
        TokenType::OAuth,
        ClientOptions {
            max_response_bytes: 20,
            ..Default::default()
        },
    )
    .unwrap();
    let e = c.submit(context(), statement()).await.unwrap_err();
    assert!(matches!(&e,Error::Concord{source,..} if source.kind()==concord::ErrorKind::Limit));
    assert_eq!(e.response_meta().unwrap().status().as_u16(), 200);
    assert_eq!(first.plan().fetch(1).await.unwrap().rows.len(), 1);
    let error_server =
        TestServer::start(|_| json(503, include_bytes!("../fixtures/rate-limit.json"))).await;
    let c = Client::new(
        &error_server.url(),
        "SYNTHETIC",
        TokenType::OAuth,
        ClientOptions {
            max_error_bytes: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let Error::Server(e) = c.submit(context(), statement()).await.unwrap_err() else {
        panic!("not server error")
    };
    assert_eq!(
        e.read_error.as_ref().unwrap().kind(),
        concord::ErrorKind::Limit
    );
    assert_eq!(e.meta.status().as_u16(), 503);
    s.shutdown().await;
    overflow.shutdown().await;
    error_server.shutdown().await;
}

#[tokio::test]
async fn bad_success_media_json_and_status_are_not_completed_queries() {
    for reply in [
        WireReply::text(200, "text/html", ONE),
        json(200, include_bytes!("../fixtures/malformed.json")),
        json(204, b""),
    ] {
        let script = Script::new([reply]);
        let s = TestServer::start(move |_| script.next()).await;
        let e = client(&s).submit(context(), statement()).await.unwrap_err();
        assert!(e.response_meta().is_some());
        assert!(matches!(
            e,
            Error::Concord { .. }
                | Error::Protocol {
                    kind: ProtocolError::UnexpectedStatus,
                    ..
                }
        ));
        s.shutdown().await;
    }
}

#[tokio::test]
async fn mismatched_handles_rejected_in_status_errors_cancel_and_partitions() {
    for (status, body) in [(202, PENDING_B), (200, COMPLETE_B), (422, NOT_FOUND)] {
        let script = Script::new([json(202, PENDING_A), json(status, body)]);
        let s = TestServer::start(move |_| script.next()).await;
        let q = client(&s)
            .submit(context(), statement())
            .await
            .unwrap()
            .query;
        assert!(matches!(
            q.status().await,
            Err(Error::Protocol {
                kind: ProtocolError::HandleMismatch,
                ..
            })
        ));
        s.shutdown().await;
    }
    let script = Script::new([
        json(200, TWO),
        json(200, &mutate(PART1, |v| v["statementHandle"] = B.into())),
        json(200, CANCELLED),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let submission = client(&s).submit(context(), statement()).await.unwrap();
    let f = first(submission.state);
    assert!(matches!(
        f.plan().fetch(1).await,
        Err(Error::Protocol {
            kind: ProtocolError::HandleMismatch,
            ..
        })
    ));
    assert!(matches!(
        submission.query.cancel(id(CANCEL_ID)).await,
        Err(Error::Protocol {
            kind: ProtocolError::HandleMismatch,
            ..
        })
    ));
    s.shutdown().await;
}

#[tokio::test]
async fn metadata_must_prove_partition_plan_and_first_rows_must_be_complete() {
    let cases: Vec<Vec<u8>> = vec![
        mutate(THREE, |v| v["resultSetMetaData"]["numRows"] = 99.into()),
        mutate(THREE, |v| {
            v["resultSetMetaData"]["partitionInfo"] = serde_json::json!([])
        }),
        mutate(THREE, |v| {
            v["resultSetMetaData"]
                .as_object_mut()
                .unwrap()
                .remove("partitionInfo");
        }),
        mutate(THREE, |v| v["resultSetMetaData"]["format"] = "arrow".into()),
        mutate(THREE, |v| v["resultSetMetaData"]["partition"] = 1.into()),
        mutate(THREE, |v| {
            v["resultSetMetaData"]["partitionInfo"][0]["rowCount"] = u64::MAX.into()
        }),
        mutate(THREE, |v| v["data"] = serde_json::json!([["truncated"]])),
        mutate(THREE, |v| v["code"] = "391908".into()),
        mutate(THREE, |v| v["statementHandles"] = serde_json::json!([A, B])),
    ];
    for body in cases {
        let s = TestServer::start(move |_| json(200, &body)).await;
        assert!(client(&s).submit(context(), statement()).await.is_err());
        assert_eq!(s.requests().len(), 1);
        s.shutdown().await;
    }
}

#[tokio::test]
async fn partition_failures_are_not_empty_success_and_out_of_range_does_not_send() {
    for body in [
        b"{}".as_slice(),
        b"{\"data\":[]}",
        b"{\"data\":[[3,4]]}",
        b"{\"data\":[[\"x\"]]}",
    ] {
        let script = Script::new([json(200, TWO), json(200, body)]);
        let s = TestServer::start(move |_| script.next()).await;
        let plan = first(
            client(&s)
                .submit(context(), statement())
                .await
                .unwrap()
                .state,
        )
        .plan();
        assert!(plan.fetch(0).await.is_err());
        assert!(plan.fetch(2).await.is_err());
        assert_eq!(s.requests().len(), 1);
        assert!(plan.fetch(1).await.is_err());
        s.shutdown().await;
    }
}

#[tokio::test]
async fn transport_truncation_is_not_success_and_keeps_operation() {
    let s = TestServer::start(|_| {
        json(200, b"").with_body(Body::FailAfter(vec![concord::bytes::Bytes::from_static(
            b"{\"code\":",
        )]))
    })
    .await;
    let e = client(&s).submit(context(), statement()).await.unwrap_err();
    let Error::Concord { source, meta } = e else {
        panic!("not body error")
    };
    assert_eq!(source.kind(), concord::ErrorKind::Transport);
    assert_eq!(meta.unwrap().operation().unwrap().name(), "SqlApi.submit");
    s.shutdown().await;
}

#[tokio::test]
async fn advisory_status_urls_are_not_followed_and_redirects_are_not_followed() {
    let script = Script::new([
        json(
            202,
            &mutate(PENDING_A, |v| {
                v["statementStatusUrl"] = "https://attacker.invalid/stolen".into()
            }),
        ),
        json(200, ONE),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let q = client(&s)
        .submit(context(), statement())
        .await
        .unwrap()
        .query;
    assert!(matches!(
        q.status().await.unwrap(),
        StatementState::Complete(_)
    ));
    assert_eq!(s.requests()[1].target, format!("/api/v2/statements/{A}"));
    s.shutdown().await;
    let s =
        TestServer::start(|_| WireReply::new(307).header("location", "http://127.0.0.1:1/never"))
            .await;
    let e = client(&s).submit(context(), statement()).await.unwrap_err();
    assert_eq!(e.response_meta().unwrap().status().as_u16(), 307);
    assert_eq!(s.requests().len(), 1);
    s.shutdown().await;
}

#[tokio::test]
async fn token_types_and_safe_configuration() {
    for (kind, wire) in [
        (TokenType::KeyPairJwt, "KEYPAIR_JWT"),
        (
            TokenType::ProgrammaticAccessToken,
            "PROGRAMMATIC_ACCESS_TOKEN",
        ),
    ] {
        let s = TestServer::start(|_| json(200, ONE)).await;
        let c = Client::new(
            &s.url(),
            "SYNTHETIC_REDACTION_SENTINEL",
            kind,
            ClientOptions::default(),
        )
        .unwrap();
        assert!(!format!("{c:?}").contains("SYNTHETIC"));
        c.submit(context(), statement()).await.unwrap();
        assert_eq!(
            s.single().header("x-snowflake-authorization-token-type"),
            Some(wire)
        );
        s.shutdown().await;
    }
    for base in [
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com/path",
        "https://example.com/?x=1",
    ] {
        assert!(
            Client::new(
                base,
                "SYNTHETIC",
                TokenType::OAuth,
                ClientOptions::default()
            )
            .is_err()
        );
    }
    assert!(RequestId::new("../unsafe").is_err());
    assert!(StatementHandle::new("../unsafe").is_err());
    assert!(
        Client::new(
            "https://example.invalid",
            "x\r\ny",
            TokenType::OAuth,
            ClientOptions::default()
        )
        .is_err()
    );
    assert!(
        Client::new(
            "https://example.invalid",
            "SYNTHETIC",
            TokenType::OAuth,
            ClientOptions {
                request_timeout: Duration::ZERO,
                ..Default::default()
            }
        )
        .is_err()
    );
}

#[tokio::test]
async fn decompressed_partition_limit_and_corrupt_gzip_are_enforced() {
    for (case, body) in [
        include_bytes!("../fixtures/partition-oversize.json.gz").as_slice(),
        b"not gzip".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let bytes = body.to_vec();
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                json(200, TWO)
            } else {
                gzip(&bytes)
            }
        })
        .await;
        let c = Client::new(
            &s.url(),
            "SYNTHETIC",
            TokenType::OAuth,
            ClientOptions {
                max_response_bytes: 1200,
                ..Default::default()
            },
        )
        .unwrap();
        let plan = first(c.submit(context(), statement()).await.unwrap().state).plan();
        let e = plan.fetch(1).await.unwrap_err();
        // Inflation now belongs to the SDK rather than reqwest's transport. Preserve the
        // limit/corruption assertions and check their distinct typed SDK causes.
        match (&e, case) {
            (
                Error::PartitionBody {
                    source: PartitionBodyError::Limit { limit: 1200 },
                    ..
                },
                0,
            )
            | (
                Error::PartitionBody {
                    source: PartitionBodyError::Gzip(_),
                    ..
                },
                1,
            ) => {}
            _ => panic!("unexpected partition error: {e:?}"),
        }
        assert_eq!(
            e.response_meta().unwrap().operation().unwrap().name(),
            "SqlApi.statement.partition"
        );
        s.shutdown().await;
    }
}

#[tokio::test]
async fn linking_sdk_does_not_enable_reqwest_decompression_for_other_clients() {
    let s = TestServer::start(|_| gzip(GZIP1)).await;
    let response = concord::reqwest::Client::new()
        .get(s.url())
        .send()
        .await
        .unwrap();
    assert_eq!(response.headers()["content-encoding"], "gzip");
    assert_eq!(response.bytes().await.unwrap().as_ref(), GZIP1);
    assert_eq!(s.single().header("accept-encoding"), None);
    s.shutdown().await;
}

#[tokio::test]
async fn partition_encoded_bytes_have_their_own_limit() {
    let s = TestServer::start(|r| {
        if r.method == "POST" {
            json(200, TWO)
        } else {
            gzip(&vec![0; 1201])
        }
    })
    .await;
    let client = Client::new(
        &s.url(),
        "SYNTHETIC",
        TokenType::OAuth,
        ClientOptions {
            max_response_bytes: 1200,
            ..Default::default()
        },
    )
    .unwrap();
    let plan = first(client.submit(context(), statement()).await.unwrap().state).plan();
    let error = plan.fetch(1).await.unwrap_err();
    assert!(
        matches!(&error, Error::Concord { source, .. } if source.kind() == concord::ErrorKind::Limit)
    );
    assert_eq!(
        error.response_meta().unwrap().headers()["content-encoding"],
        "gzip"
    );
    s.shutdown().await;
}

#[tokio::test]
async fn partition_gzip_requires_valid_trailers_and_rejects_trailing_junk() {
    let mut bad_crc = GZIP1.to_vec();
    let crc_at = bad_crc.len() - 8;
    bad_crc[crc_at] ^= 1;
    let mut trailing_junk = GZIP1.to_vec();
    trailing_junk.extend_from_slice(b"junk");
    for bytes in [bad_crc, GZIP1[..GZIP1.len() - 1].to_vec(), trailing_junk] {
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                json(200, TWO)
            } else {
                gzip(&bytes)
            }
        })
        .await;
        let plan = first(
            client(&s)
                .submit(context(), statement())
                .await
                .unwrap()
                .state,
        )
        .plan();
        let error = plan.fetch(1).await.unwrap_err();
        assert!(matches!(
            error,
            Error::PartitionBody {
                source: PartitionBodyError::Gzip(_),
                ..
            }
        ));
        s.shutdown().await;
    }
}

#[tokio::test]
async fn exact_inflation_limit_and_multiple_gzip_members_are_supported() {
    use std::io::Write;
    fn compress(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }
    let mut padded = PART1.to_vec();
    padded.resize(1200, b' ');
    let mut members = compress(&padded[..600]);
    members.extend_from_slice(&compress(&padded[600..]));
    for bytes in [compress(&padded), members] {
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                json(200, TWO)
            } else {
                gzip(&bytes)
            }
        })
        .await;
        let client = Client::new(
            &s.url(),
            "SYNTHETIC",
            TokenType::OAuth,
            ClientOptions {
                max_response_bytes: 1200,
                ..Default::default()
            },
        )
        .unwrap();
        let plan = first(client.submit(context(), statement()).await.unwrap().state).plan();
        assert_eq!(
            plan.fetch(1).await.unwrap().rows[0][1].as_deref(),
            Some("βeta")
        );
        s.shutdown().await;
    }
}

#[tokio::test]
async fn partition_content_encoding_is_checked_before_reading_the_body() {
    for encodings in [vec!["br"], vec!["gzip, gzip"], vec!["gzip", "gzip"]] {
        let s = TestServer::start(move |r| {
            if r.method == "POST" {
                return json(200, TWO);
            }
            let mut reply = json(200, b"").with_body(Body::Stall);
            for encoding in &encodings {
                reply = reply.header("content-encoding", *encoding);
            }
            reply
        })
        .await;
        let plan = first(
            client(&s)
                .submit(context(), statement())
                .await
                .unwrap()
                .state,
        )
        .plan();
        let error = plan.fetch(1).await.unwrap_err();
        assert!(
            matches!(&error, Error::Concord { source, .. } if source.kind() == concord::ErrorKind::Metadata)
        );
        assert_eq!(
            error.response_meta().unwrap().operation().unwrap().name(),
            "SqlApi.statement.partition"
        );
        s.shutdown().await;
    }
}

#[tokio::test]
async fn success_from_cancel_requires_documented_code_and_sqlstate() {
    let script = Script::new([
        json(202, PENDING_B),
        json(200, &mutate(CANCELLED, |v| v["code"] = "090001".into())),
    ]);
    let s = TestServer::start(move |_| script.next()).await;
    let q = client(&s)
        .submit(context(), statement())
        .await
        .unwrap()
        .query;
    assert!(matches!(
        q.cancel(id(CANCEL_ID)).await,
        Err(Error::Protocol {
            kind: ProtocolError::UnexpectedCancellation,
            ..
        })
    ));
    s.shutdown().await;
}
