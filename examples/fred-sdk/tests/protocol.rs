//! Independent wire, error, bounds and redaction checks against a local HTTP server.

mod support;

use concord::ErrorKind;
use concord_example_testserver::{Body, Recorded, Reply, TestServer, paused};
use concord_sdk_fred::{
    BODY_LIMIT, ERROR_BODY_LIMIT, Error, Fred, FredConfig, ObservationLimit, SearchLimit, concord,
};
use support::{KEY, api, kind, safe};

const XML: &[u8] = include_bytes!("../fixtures/observations-0.xml");
const JSON: &[u8] = include_bytes!("../fixtures/observations-0.json");
const ZIP: &[u8] = include_bytes!("../fixtures/observations.zip");

#[tokio::test]
async fn generated_wire_shapes_and_query_encoding() {
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.method, "GET");
        assert_eq!(r.query_all("api_key"), [KEY]);
        assert!(r.body.is_empty());
        assert_eq!(r.header("content-type"), None);
        assert_eq!(r.header("authorization"), None);
        match (
            r.path(),
            r.query_all("file_type").first().map(String::as_str),
        ) {
            ("/fred/series/search", None) => {
                Reply::text(200, "text/xml", include_bytes!("../fixtures/search-0.xml"))
            }
            ("/fred/series/search", Some("json")) => {
                Reply::json(200, include_bytes!("../fixtures/search-0.json"))
            }
            ("/fred/series", None) => {
                Reply::text(200, "text/xml", include_bytes!("../fixtures/metadata.xml"))
            }
            ("/fred/series", Some("json")) => {
                Reply::json(200, include_bytes!("../fixtures/metadata.json"))
            }
            ("/fred/series/observations", None) => Reply::text(200, "text/xml", XML),
            ("/fred/series/observations", Some("json")) => Reply::json(200, JSON),
            ("/fred/series/observations", Some("csv")) => Reply::text(200, "application/zip", ZIP),
            _ => Reply::new(404),
        }
    })
    .await;
    let client = api(&server);
    Fred::validate_header_names().unwrap();
    client
        .search("activity & output/é")
        .set_limit(SearchLimit::new(1).unwrap())
        .set_offset(3)
        .send()
        .await
        .unwrap();
    client.search_json("activity").send().await.unwrap();
    let selected = client.series("SYN_A/part &é");
    selected.metadata().send().await.unwrap();
    selected.metadata_json().send().await.unwrap();
    selected
        .observations()
        .set_limit(ObservationLimit::new(2).unwrap())
        .set_offset(7)
        .send()
        .await
        .unwrap();
    selected.observations_json().send().await.unwrap();
    selected.export_csv().set_offset(2).send().await.unwrap();
    let seen = server.requests();
    assert_eq!(seen.len(), 7);
    assert!(
        seen[0]
            .target
            .contains("search_text=activity+%26+output%2F%C3%A9")
    );
    assert_eq!(seen[0].query_all("limit"), ["1"]);
    assert_eq!(seen[0].query_all("offset"), ["3"]);
    assert_eq!(seen[0].header("accept"), Some("text/xml"));
    assert_eq!(seen[1].header("accept"), Some("application/json"));
    assert_eq!(seen[2].query_all("series_id"), ["SYN_A/part &é"]);
    assert_eq!(seen[2].query_all("limit"), Vec::<String>::new());
    assert_eq!(seen[4].query_all("limit"), ["2"]);
    assert_eq!(seen[4].query_all("offset"), ["7"]);
    assert_eq!(seen[5].query_all("limit"), ["100000"]);
    assert_eq!(seen[6].header("accept"), Some("application/zip"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn handles_prepared_requests_and_errors_redact_query_key() {
    let server = TestServer::start(|_: &Recorded| Reply::hang_up()).await;
    let config = FredConfig {
        api_key: KEY.into(),
    };
    assert!(!format!("{config:?}").contains(KEY));
    let client = api(&server);
    let scope = client.series("SYN_A");
    let op = scope.observations();
    for shown in [
        format!("{client:?}"),
        format!("{scope:?}"),
        format!("{op:?}"),
        format!("{:?}", op.clone().prepare().unwrap()),
    ] {
        assert!(!shown.contains(KEY));
    }
    let error = op.send().await.unwrap_err();
    assert_eq!(kind(&error), ErrorKind::Transport);
    safe(&error);
    assert_eq!(server.requests().len(), 1);
    let build_error = client
        .with_base_url(format!("http://user:{KEY}@invalid.test"))
        .unwrap_err();
    assert_eq!(build_error.kind(), ErrorKind::Build);
    assert!(!format!("{build_error} {build_error:?}").contains(KEY));
    server.shutdown().await;
}

#[test]
fn page_size_ranges_reject_before_io() {
    for value in [0, 1001, u32::MAX] {
        safe(&SearchLimit::new(value).unwrap_err());
    }
    for value in [0, 100001, u32::MAX] {
        safe(&ObservationLimit::new(value).unwrap_err());
    }
    assert_eq!(SearchLimit::new(1).unwrap().get(), 1);
    assert_eq!(SearchLimit::default().get(), 1000);
    assert_eq!(ObservationLimit::default().get(), 100000);
}

#[tokio::test]
async fn every_documented_http_error_class_with_xml_and_json() {
    for status in [400, 404, 423, 429, 500, 503] {
        for json in [false, true] {
            let server = TestServer::start(move |_: &Recorded| {
                let reply = if json {
                    Reply::json(status, include_bytes!("../fixtures/error.json"))
                } else {
                    Reply::text(
                        status,
                        "text/xml;charset=utf-8",
                        include_bytes!("../fixtures/error.xml"),
                    )
                };
                reply
                    .header("retry-after", "17")
                    .header("x-synthetic-secret", KEY)
            })
            .await;
            let error = api(&server)
                .series("SYN_A")
                .export_csv()
                .send()
                .await
                .unwrap_err();
            safe(&error);
            let Error::Http(http) = error else {
                panic!("HTTP failure lost")
            };
            assert_eq!(http.meta.status().as_u16(), status);
            assert_eq!(
                http.retry_after,
                Ok(Some(std::time::Duration::from_secs(17)))
            );
            let problem = http.document.unwrap();
            assert_eq!(problem.error_code, 400); // Synthetic mismatch must not replace HTTP status.
            assert!(problem.message_unredacted().contains(KEY));
            assert!(!format!("{problem:?}").contains(KEY));
            assert_eq!(server.requests().len(), 1); // No status retry.
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn retry_after_metadata_uses_shared_strict_parser_without_retrying() {
    use concord_sdk_fred::concord_retry::http::RetryAfterError;
    for scenario in 0..5 {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = Reply::json(429, include_bytes!("../fixtures/error.json"));
            match scenario {
                0 => reply,
                1 => reply.header("retry-after", "1").header("retry-after", "2"),
                2 => reply.header("retry-after", KEY),
                3 => reply.header("retry-after", "18446744073709551616"),
                _ => reply.header("retry-after", "Sun, 06 Nov 1994 08:49:37 GMT"),
            }
        })
        .await;
        let error = api(&server).search("synthetic").send().await.unwrap_err();
        safe(&error);
        let Error::Http(http) = error else {
            unreachable!()
        };
        let expected = match scenario {
            0 => Ok(None),
            1 => Err(RetryAfterError::Repeated),
            2 => Err(RetryAfterError::Invalid),
            3 => Err(RetryAfterError::Overflow),
            _ => Ok(Some(std::time::Duration::ZERO)),
        };
        assert_eq!(http.retry_after, expected);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn http_status_survives_bad_error_documents() {
    for scenario in 0..6 {
        let server = TestServer::start(move |_: &Recorded| match scenario {
            0 => Reply::json(429, b"{"),
            1 => Reply::new(429),
            2 => Reply::text(429, "text/html", KEY),
            3 => Reply::json(429, vec![b' '; ERROR_BODY_LIMIT + 1]),
            4 => Reply::json(429, b"").with_body(Body::FailAfter(vec![
                concord::bytes::Bytes::from_static(b"{"),
            ])),
            _ => Reply::text(429, "text/xml", include_bytes!("../fixtures/error.json")),
        })
        .await;
        let error = api(&server).search("synthetic").send().await.unwrap_err();
        safe(&error);
        let Error::Http(http) = error else {
            panic!("HTTP failure lost")
        };
        assert_eq!(http.meta.status().as_u16(), 429);
        let expected = [
            ErrorKind::Decode,
            ErrorKind::Metadata,
            ErrorKind::Metadata,
            ErrorKind::Limit,
            ErrorKind::Transport,
        ];
        if scenario < 5 {
            assert_eq!(http.document.unwrap_err().kind(), expected[scenario]);
        } else {
            assert_eq!(http.document.unwrap().error_code, 400);
        }
        server.shutdown().await;
    }
}

#[tokio::test]
async fn wrong_missing_duplicate_and_invalid_media_refused_before_read() {
    for scenario in 0..7 {
        let server = TestServer::start(move |_: &Recorded| {
            let reply = match scenario {
                0 => Reply::new(200),
                1 => Reply::new(200).header("content-type", "text/html"),
                2 => Reply::new(200)
                    .header("content-type", "text/xml")
                    .header("content-type", "text/xml"),
                3 => Reply::new(200).header("content-type", "text/xml; charset=iso-8859-1"),
                4 => {
                    Reply::new(200).header("content-type", "text/xml; charset=utf-8; charset=utf-8")
                }
                5 => Reply::new(200).header("content-type", "not a mime type"),
                _ => Reply::new(200).header("content-type", "application/json"),
            };
            reply.with_body(Body::Stall)
        })
        .await;
        // Clock frozen while the call runs: the bound fails only a call that waits for something
        // that never comes (then reported as stuck).
        let error = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            paused::frozen(api(&server).series("SYN_A").observations().send()),
        )
        .await
        .expect("head rejection must not read the stalled body")
        .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Metadata);
        safe(&error);
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| Reply::text(200, "text/xml", JSON)).await;
    assert_eq!(
        kind(
            &api(&server)
                .series("SYN_A")
                .observations_json()
                .send()
                .await
                .unwrap_err()
        ),
        ErrorKind::Metadata
    );
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_truncated_and_wrong_shape_documents() {
    let cases: &[(&[u8], bool)] = &[
        (include_bytes!("../fixtures/malformed.xml"), false),
        (include_bytes!("../fixtures/dtd.xml"), false),
        (b"<wrong/>", false), (b"\xff", false),
        (b"<observations/>", false),
        (b"<observations count=\"1\" offset=\"0\" limit=\"1\"><wrong/></observations>", false),
        (include_bytes!("../fixtures/malformed.json"), true),
        (b"{}", true),
        (b"{\"count\":1,\"offset\":0,\"limit\":1,\"observations\":[{\"date\":\"2000-01-01\",\"value\":1}]}", true),
    ];
    for &(body, json) in cases {
        let body = body.to_vec();
        let server = TestServer::start(move |_: &Recorded| {
            if json {
                Reply::json(200, &body)
            } else {
                Reply::text(200, "text/xml", &body)
            }
        })
        .await;
        let scope = api(&server).series("SYN_A");
        let error = if json {
            scope.observations_json().send().await.unwrap_err()
        } else {
            scope.observations().send().await.unwrap_err()
        };
        assert_eq!(kind(&error), ErrorKind::Decode);
        safe(&error);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn body_limits_apply_without_content_length_and_transport_keeps_head() {
    for format in ["xml", "json", "zip"] {
        for truncated in [false, true] {
            let server = TestServer::start(move |_: &Recorded| {
                let media = match format {
                    "xml" => "text/xml",
                    "json" => "application/json",
                    _ => "application/zip",
                };
                let body = if truncated {
                    Body::FailAfter(vec![concord::bytes::Bytes::from_static(b"prefix")])
                } else {
                    Body::Chunks(vec![(
                        std::time::Duration::ZERO,
                        concord::bytes::Bytes::from(if format == "zip" {
                            support::oversized_archive()
                        } else {
                            vec![b'x'; BODY_LIMIT + 1]
                        }),
                    )])
                };
                Reply::text(200, media, b"").with_body(body)
            })
            .await;
            let scope = api(&server).series("SYN_A");
            let error = match format {
                "xml" => scope.observations().send().await.unwrap_err(),
                "json" => scope.observations_json().send().await.unwrap_err(),
                _ => scope.export_csv().send().await.unwrap_err(),
            };
            assert_eq!(
                kind(&error),
                if truncated {
                    ErrorKind::Transport
                } else {
                    ErrorKind::Limit
                }
            );
            safe(&error);
            let Error::Concord(inner) = error else {
                unreachable!()
            };
            assert_eq!(inner.head().unwrap().status().as_u16(), 200);
            server.shutdown().await;
        }
    }
}

#[tokio::test]
async fn invalid_archives_are_never_plain_csv_or_partially_successful() {
    let cases: &[&[u8]] = &[
        include_bytes!("../fixtures/observations.csv"),
        include_bytes!("../fixtures/truncated.zip"),
        include_bytes!("../fixtures/malformed.zip"),
        include_bytes!("../fixtures/oversized-extracted.zip"),
        include_bytes!("../fixtures/oversized-total.zip"),
        include_bytes!("../fixtures/too-many-entries.zip"),
        include_bytes!("../fixtures/ambiguous.zip"),
        include_bytes!("../fixtures/no-csv.zip"),
        include_bytes!("../fixtures/unsafe-path.zip"),
        include_bytes!("../fixtures/unexpected-member.zip"),
        include_bytes!("../fixtures/symlink.zip"),
        include_bytes!("../fixtures/bad-crc.zip"),
        include_bytes!("../fixtures/encrypted.zip"),
        include_bytes!("../fixtures/unsupported-method.zip"),
        include_bytes!("../fixtures/forged-size.zip"),
    ];
    for &body in cases {
        let server =
            TestServer::start(move |_: &Recorded| Reply::text(200, "application/zip", body)).await;
        let error = api(&server)
            .series("SYN_A")
            .export_csv()
            .send()
            .await
            .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Decode);
        safe(&error);
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| Reply::text(200, "text/csv", ZIP)).await;
    assert_eq!(
        kind(
            &api(&server)
                .series("SYN_A")
                .export_csv()
                .send()
                .await
                .unwrap_err()
        ),
        ErrorKind::Metadata
    );
    server.shutdown().await;
}

#[tokio::test]
async fn accepted_xml_and_zip_media_preserve_exact_strings_and_bytes() {
    for media in ["text/xml; charset=utf-8", "application/xml"] {
        let server = TestServer::start(move |_: &Recorded| Reply::text(200, media, XML)).await;
        let page = api(&server)
            .series("SYN_A")
            .observations()
            .send()
            .await
            .unwrap();
        assert_eq!(page.observations[0].value, "2.5000");
        assert_eq!(page.observations[1].value, ".");
        server.shutdown().await;
    }
    for media in [
        "application/zip",
        "application/x-zip-compressed",
        "application/octet-stream",
    ] {
        let server = TestServer::start(move |_: &Recorded| {
            Reply::text(200, media, include_bytes!("../fixtures/stored.zip"))
        })
        .await;
        let export = api(&server)
            .series("SYN_A")
            .export_csv()
            .send()
            .await
            .unwrap();
        assert_eq!(export.csv(), include_bytes!("../fixtures/observations.csv"));
        assert_eq!(export.readme(), None);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn redirect_is_returned_without_forwarding_key() {
    let destination = TestServer::start(|_: &Recorded| Reply::new(500)).await;
    let target = destination.url();
    let server = TestServer::start(move |_: &Recorded| {
        Reply::new(302).header("location", format!("{target}/?api_key={KEY}"))
    })
    .await;
    let error = api(&server).search("synthetic").send().await.unwrap_err();
    safe(&error);
    assert!(matches!(error, Error::Http(ref h) if h.meta.status().as_u16() == 302));
    assert!(destination.requests().is_empty());
    server.shutdown().await;
    destination.shutdown().await;
}
