//! Protocol contracts are independently specified in PROVENANCE and fixtures/README.md.
//! Fault cases below are authored synthetic adversarial responses, never live observations.
mod support;
use concord_sdk_europe_pmc::{
    BODY_LIMIT, Cursor, ERROR_BODY_LIMIT, Error, EuropePmc, PageSize, PublicationId, Query,
    RecordKey, SearchFormat, Source,
    concord::{ErrorKind, bytes::Bytes, reqwest},
    concord_pagination::Resume,
    concord_retry::http::RetryAfterError,
};
use concord_testserver::{Body, Recorded, Reply, TestServer, paused};
use std::{
    num::NonZeroUsize,
    time::{Duration, UNIX_EPOCH},
};
use support::*;

fn kind(error: &Error) -> ErrorKind {
    error.call_error().unwrap().kind()
}

#[tokio::test]
async fn wire_encoding_singletons_headers_and_scoped_snapshots() {
    EuropePmc::validate_header_names().unwrap();
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::json(200, EMPTY_JSON) }).await;
    let api = api(&server);
    let original = api.search(
        Query::new("TITLE:\"café + tea\" & year:2020").unwrap(),
        PageSize::new(2).unwrap(),
    );
    let changed = original.clone().set_query(Query::new("malaria").unwrap());
    original
        .json()
        .set_cursor(Cursor::new("a+/=?&").unwrap())
        .send()
        .await
        .unwrap();
    changed.json().send().await.unwrap();
    let wire = server.requests();
    let r = &wire[0];
    assert_eq!(r.method, "GET");
    assert_eq!(r.path(), "/rest/search");
    assert_eq!(
        r.query_pairs(),
        [
            ("query".into(), "TITLE:\"café + tea\" & year:2020".into()),
            ("pageSize".into(), "2".into()),
            ("cursorMark".into(), "a+/=?&".into()),
            ("format".into(), "json".into()),
            ("resultType".into(), "lite".into()),
        ]
    );
    assert!(r.target.contains("caf%C3%A9"));
    assert!(r.target.contains("cursorMark=a%2B%2F%3D%3F%26"));
    assert_eq!(r.header("accept"), Some("application/json"));
    assert!(r.header("content-type").is_none());
    assert!(r.header("authorization").is_none());
    assert!(r.body.is_empty());
    assert_eq!(wire[1].query_all("query"), ["malaria"]);
    assert_eq!(wire[1].query_all("cursorMark"), ["*"]);
    server.shutdown().await;
}

#[tokio::test]
async fn local_validation_and_build_errors_make_no_request() {
    let server = fixed(200, "application/json", EMPTY_JSON).await;
    let api = api(&server);
    for value in ["", " ", "line\nbreak", "nul\0"] {
        redacted(&Query::new(value).unwrap_err());
        assert!(matches!(Cursor::new(value), Err(Error::InvalidInput(_))));
    }
    assert!(Query::new("q".repeat(2049)).is_err());
    assert!(Cursor::new("c".repeat(4097)).is_err());
    for value in ["", "med", "MED OR PMC", "MED/"] {
        assert!(Source::new(value).is_err());
    }
    for value in ["", ".", "..", "a/b", "1 OR *:*", "x\"", SECRET] {
        // SECRET uses hyphens, so is a valid identifier; query operators and path separators are not.
        if value == SECRET {
            assert!(PublicationId::new(value).is_ok());
        } else {
            assert!(PublicationId::new(value).is_err());
        }
    }
    for n in [0, 101, u16::MAX] {
        assert!(matches!(PageSize::new(n), Err(Error::InvalidInput(_))));
    }
    assert_eq!(PageSize::default().get(), 25);
    let cap = NonZeroUsize::new(21).unwrap();
    assert!(matches!(
        search(&api).pages(SearchFormat::Xml, Resume::Start, cap),
        Err(Error::InvalidInput(_))
    ));
    let build = Error::from(
        api.with_base_url(format!("{}?token={SECRET}", server.url()))
            .unwrap_err(),
    );
    assert_eq!(kind(&build), ErrorKind::Build);
    redacted(&build);
    assert_eq!(server.requests().len(), 0);
    server.shutdown().await;
}

#[tokio::test]
async fn strict_media_types_missing_repeated_malformed_and_wrong_charset() {
    for xml in [false, true] {
        for media in [
            None,
            Some("text/html"),
            Some("application/json, application/xml"),
            Some("application/*"),
            Some("text/"),
            Some(if xml {
                "application/xml;charset=iso-8859-1"
            } else {
                "application/json;charset=iso-8859-1"
            }),
            Some(if xml {
                "application/json"
            } else {
                "application/xml"
            }),
            Some(if xml {
                "application/xml;charset=utf-8;charset=utf-8"
            } else {
                "application/json;charset=utf-8;charset=utf-8"
            }),
        ] {
            let server = TestServer::start(move |_: &Recorded| -> Reply {
                let reply = Reply::new(200).with_body(Body::Stall);
                if let Some(media) = media {
                    reply.header("content-type", media)
                } else {
                    reply
                }
            })
            .await;
            let api = api(&server);
            // The stalled body proves media refusal happens before any body parsing/read.
            let call = async {
                if xml {
                    search(&api).xml().send().await
                } else {
                    search(&api).json().send().await
                }
            };
            // Clock frozen while the call runs: the bound fails only a call that waits for something
            // that never comes (then reported as stuck).
            let error = tokio::time::timeout(Duration::from_secs(1), paused::frozen(call))
                .await
                .unwrap()
                .unwrap_err();
            assert_eq!(kind(&error), ErrorKind::Metadata, "{media:?}, XML={xml}");
            assert_eq!(error.status().unwrap().as_u16(), 200);
            assert!(error.call_error().unwrap().response().is_some());
            redacted(&error);
            server.shutdown().await;
        }
        let media = if xml {
            "application/xml"
        } else {
            "application/json"
        };
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::new(200)
                .header("content-type", media)
                .header("content-type", media)
                .body(EMPTY_JSON)
        })
        .await;
        let api = api(&server);
        let error = if xml {
            search(&api).xml().send().await
        } else {
            search(&api).json().send().await
        }
        .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Metadata);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn accepted_xml_and_json_media_and_xml_entities_are_decoded() {
    for media in [
        "application/xml",
        "text/xml",
        "Application/XML; Charset=\"UTF-8\"; x=y",
    ] {
        let server = fixed(200, media, FIRST_XML).await;
        let api = api(&server);
        let page = search(&api).xml().send().await.unwrap();
        assert_eq!(page.next_cursor_mark.unwrap().as_str(), "p+1/==?&");
        server.shutdown().await;
    }
    for media in [
        "application/json",
        "application/vnd.fixture+json; charset=us-ascii",
    ] {
        let server = fixed(200, media, FIRST_JSON).await;
        assert_eq!(
            search(&api(&server))
                .json()
                .send()
                .await
                .unwrap()
                .result_list
                .result
                .len(),
            2
        );
        server.shutdown().await;
    }
}

#[tokio::test]
async fn malformed_truncated_wrong_shape_and_non_utf8_bodies_fail_safely() {
    let jsons = [
        include_str!("../fixtures/malformed.json"),
        "",
        "null",
        "{}",
        "{\"hitCount\":1,\"resultList\":{\"result\":[{}]}}",
        FIRST_XML,
    ];
    for body in jsons {
        let server = fixed(200, "application/json", body).await;
        let error = search(&api(&server)).json().send().await.unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Decode);
        assert_eq!(error.status().unwrap().as_u16(), 200);
        redacted(&error);
        server.shutdown().await;
    }
    let xmls = [
        include_str!("../fixtures/malformed.xml"),
        "",
        "<wrong/>",
        FIRST_JSON,
        "<responseWrapper><hitCount>0</hitCount></responseWrapper>",
        "<responseWrapper><hitCount>0</hitCount><hitCount>0</hitCount><resultList/></responseWrapper>",
        "<responseWrapper><hitCount>+1</hitCount><resultList/></responseWrapper>",
        "<responseWrapper><hitCount>18446744073709551616</hitCount><resultList/></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><resultList><result><id>1</id></result></resultList></responseWrapper>",
        "<responseWrapper><hitCount>1</hitCount><resultList><result><id>1</id><source>MED</source><title><b>nested</b></title></result></resultList></responseWrapper>",
        "<!DOCTYPE responseWrapper [<!ENTITY hidden SYSTEM 'http://127.0.0.1:9/no-fetch'>]><responseWrapper><hitCount>0</hitCount><resultList>&hidden;</resultList></responseWrapper>",
    ];
    for body in xmls {
        let server = fixed(200, "application/xml", body).await;
        let error = search(&api(&server)).xml().send().await.unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Decode, "{body}");
        redacted(&error);
        server.shutdown().await;
    }
    for xml in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(
                200,
                if xml { "text/xml" } else { "application/json" },
                [0xff, 0xfe],
            )
        })
        .await;
        let api = api(&server);
        let error = if xml {
            search(&api).xml().send().await
        } else {
            search(&api).json().send().await
        }
        .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Decode);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn byte_limits_apply_to_both_formats_and_native_prepared_send() {
    for xml in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::new(200)
                .header(
                    "content-type",
                    if xml {
                        "application/xml"
                    } else {
                        "application/json"
                    },
                )
                .with_body(Body::Chunks(vec![(
                    Duration::ZERO,
                    Bytes::from(vec![b' '; BODY_LIMIT + 1]),
                )]))
        })
        .await;
        let api = api(&server);
        let error = if xml {
            search(&api).xml().send().await
        } else {
            search(&api).json().send().await
        }
        .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Limit);
        assert_eq!(error.status().unwrap().as_u16(), 200);
        let native = search(&api)
            .json()
            .prepare()
            .unwrap()
            .send()
            .await
            .unwrap_err();
        assert_eq!(
            native.kind(),
            if xml {
                ErrorKind::Metadata
            } else {
                ErrorKind::Limit
            }
        );
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| -> Reply {
        let mut body = EMPTY_JSON.as_bytes().to_vec();
        body.resize(BODY_LIMIT, b' ');
        Reply::json(200, body)
    })
    .await;
    search(&api(&server)).json().send().await.unwrap(); // exact byte boundary succeeds
    server.shutdown().await;
}

#[tokio::test]
async fn http_status_is_authoritative_for_all_failure_classes_and_never_retries() {
    for status in [301, 302, 400, 401, 403, 404, 408, 422, 429, 500, 503] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::json(status, include_str!("../fixtures/provider-error.json"))
                .header("retry-after", "2")
                .header("set-cookie", SECRET)
                .header("location", format!("http://127.0.0.1:9/{SECRET}"))
        })
        .await;
        let error = search(&api(&server)).json().send().await.unwrap_err();
        assert_eq!(error.status().unwrap().as_u16(), status);
        let Error::Http(http) = &error else {
            panic!("expected HTTP failure")
        };
        assert!(http.body.as_ref().unwrap().ends_with(b"}\n"));
        assert_eq!(
            http.retry_after(UNIX_EPOCH).unwrap(),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            http.meta.operation().unwrap().name(),
            "EuropePmc.search.json"
        );
        redacted(&error);
        assert!(!format!("{http:?}").contains(SECRET));
        assert!(std::error::Error::source(&error).is_none());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn unreadable_http_error_bodies_preserve_status_and_redaction() {
    for limit in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let body = if limit {
                Body::Chunks(vec![(
                    Duration::ZERO,
                    Bytes::from(vec![b'x'; ERROR_BODY_LIMIT + 1]),
                )])
            } else {
                Body::FailAfter(vec![Bytes::from_static(b"synthetic-SECRET-DO-NOT-LOG")])
            };
            Reply::new(429)
                .header("content-type", "text/html")
                .with_body(body)
        })
        .await;
        let error = search(&api(&server)).json().send().await.unwrap_err();
        let Error::Http(http) = &error else {
            panic!("expected HTTP failure")
        };
        assert_eq!(
            http.body.as_ref().unwrap_err().kind(),
            if limit {
                ErrorKind::Limit
            } else {
                ErrorKind::Transport
            }
        );
        assert_eq!(error.status().unwrap().as_u16(), 429);
        redacted(&error);
        server.shutdown().await;
    }
    // Error documents of unknown/malformed media are raw diagnostics, not success decodes.
    let server = fixed(400, "text/html", "<unclosed>synthetic-SECRET-DO-NOT-LOG").await;
    let error = search(&api(&server)).xml().send().await.unwrap_err();
    assert!(matches!(&error, Error::Http(e) if e.body.is_ok()));
    redacted(&error);
    server.shutdown().await;
}

#[tokio::test]
async fn retry_after_absence_dates_repetition_invalid_and_overflow_are_explicit() {
    for (values, expected) in [
        (vec![], Ok(None)),
        (
            vec!["Thu, 01 Jan 1970 00:00:10 GMT"],
            Ok(Some(Duration::from_secs(10))),
        ),
        (vec!["-1"], Err(RetryAfterError::Invalid)),
        (vec![SECRET], Err(RetryAfterError::Invalid)),
        (vec!["1", "2"], Err(RetryAfterError::Repeated)),
        (vec!["18446744073709551616"], Err(RetryAfterError::Overflow)),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            let mut reply = Reply::new(503);
            for value in &values {
                reply = reply.header("retry-after", *value);
            }
            reply
        })
        .await;
        let error = search(&api(&server)).json().send().await.unwrap_err();
        let Error::Http(http) = &error else {
            panic!("expected HTTP failure")
        };
        assert_eq!(http.retry_after(UNIX_EPOCH), expected);
        redacted(&error);
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn headless_mid_body_and_timeout_transport_failures_are_distinct_from_decode() {
    let server = TestServer::start(|_: &Recorded| -> Reply { Reply::hang_up() }).await;
    let error = search(&api(&server)).json().send().await.unwrap_err();
    assert_eq!(kind(&error), ErrorKind::Transport);
    assert!(error.status().is_none());
    redacted(&error);
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
    for xml in [false, true] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::new(200)
                .header(
                    "content-type",
                    if xml {
                        "application/xml"
                    } else {
                        "application/json"
                    },
                )
                .with_body(Body::FailAfter(vec![Bytes::from_static(
                    b"synthetic-SECRET-DO-NOT-LOG",
                )]))
        })
        .await;
        let api = api(&server);
        let error = if xml {
            search(&api).xml().send().await
        } else {
            search(&api).json().send().await
        }
        .unwrap_err();
        assert_eq!(kind(&error), ErrorKind::Transport);
        assert_eq!(error.status().unwrap().as_u16(), 200);
        redacted(&error);
        server.shutdown().await;
    }
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Stall)
    })
    .await;
    let client = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .timeout(Duration::from_millis(50))
        .build()
        .unwrap();
    let api = EuropePmc::new(client).with_base_url(server.url()).unwrap();
    let error = search(&api).json().send().await.unwrap_err();
    assert_eq!(kind(&error), ErrorKind::Transport);
    assert!(
        error
            .call_error()
            .unwrap()
            .transport()
            .unwrap()
            .is_timeout()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn malformed_envelopes_and_exact_citation_selection_are_not_silent_empty_successes() {
    let key = RecordKey::new("MED", "30000001").unwrap();
    let server = fixed(200, "application/json", EMPTY_JSON).await;
    assert!(matches!(
        api(&server).fetch_citation(&key).await,
        Err(Error::NotFound)
    ));
    server.shutdown().await;
    let server = fixed(200, "application/json", LAST_JSON).await;
    assert!(matches!(
        api(&server).fetch_citation(&key).await,
        Err(Error::UnexpectedRecord)
    ));
    server.shutdown().await;
    let server = fixed(
        200,
        "application/json",
        "{\"hitCount\":0,\"resultList\":{\"result\":[{\"id\":\"1\",\"source\":\"MED\"}]}}",
    )
    .await;
    let error = search(&api(&server)).json().send().await.unwrap_err();
    assert!(matches!(error, Error::InvalidPage(_)));
    redacted(&error);
    server.shutdown().await;
    let server = fixed(200, "application/json", FIRST_JSON).await;
    assert!(matches!(
        api(&server).fetch_citation(&key).await,
        Err(Error::InvalidPage(_))
    )); // requested size 1
    server.shutdown().await;
    let server = fixed(
        200,
        "application/json",
        "{\"hitCount\":1,\"citationList\":{\"citation\":[{\"id\":\"9\",\"source\":\"MED\"}]}}",
    )
    .await;
    let page = api(&server)
        .publication(key.source.clone(), key.id.clone())
        .citations()
        .send()
        .await
        .unwrap();
    assert_eq!(page.citation_list.citation[0].id.as_str(), "9");
    server.shutdown().await;
}

#[tokio::test]
async fn secrets_are_withheld_by_generated_handles_models_and_error_formatters() {
    let server = fixed(
        200,
        "application/json",
        include_str!("../fixtures/malformed.json"),
    )
    .await;
    let api = api(&server);
    let scope = api.search(Query::new(SECRET).unwrap(), PageSize::default());
    let op = scope.json().set_cursor(Cursor::new(SECRET).unwrap());
    for debug in [
        format!("{api:?}"),
        format!("{scope:?}"),
        format!("{op:?}"),
        format!("{:?}", op.clone().prepare().unwrap()),
    ] {
        assert!(!debug.contains(SECRET));
    }
    let error = op.send().await.unwrap_err();
    redacted(&error);
    assert!(format!("{error}").contains("EuropePmc.search.json"));
    assert!(!format!("{:?}", Query::new(SECRET).unwrap()).contains(SECRET));
    assert!(!format!("{:?}", Cursor::new(SECRET).unwrap()).contains(SECRET));
    server.shutdown().await;
}

#[tokio::test]
async fn xml_comments_are_not_cursor_text_and_model_debug_withholds_content() {
    // Synthetic scalar XML: XML comments are not character data, CDATA and &amp; are.
    let body = "<responseWrapper xmlns:epmc='https://www.europepmc.org/data'><hitCount>1</hitCount><nextCursorMark>p<!--synthetic-SECRET-DO-NOT-LOG-->+1/==?&amp;</nextCursorMark><resultList><result><id>1</id><source>MED</source><title><![CDATA[synthetic-SECRET-DO-NOT-LOG]]> &amp; title</title><extra>ignored</extra></result></resultList></responseWrapper>";
    let server = fixed(200, "application/xml", body).await;
    let page = search(&api(&server)).xml().send().await.unwrap();
    assert_eq!(page.next_cursor_mark.as_ref().unwrap().as_str(), "p+1/==?&");
    assert_eq!(
        page.result_list.result[0].title.as_deref(),
        Some("synthetic-SECRET-DO-NOT-LOG & title")
    );
    assert!(!format!("{page:?}").contains(SECRET));
    server.shutdown().await;

    // A repeated initial marker is also a failure when its page is empty.
    let server = fixed(
        200,
        "application/json",
        "{\"hitCount\":0,\"nextCursorMark\":\"*\",\"resultList\":{\"result\":[]}}",
    )
    .await;
    let mut pages = search(&api(&server))
        .pages(
            SearchFormat::Json,
            Resume::Start,
            NonZeroUsize::new(2).unwrap(),
        )
        .unwrap();
    let error = pages.next_page().await.unwrap_err();
    assert!(matches!(
        error.provider_error(),
        Some(Error::RepeatedCursor)
    ));
    assert_eq!(server.requests().len(), 1);
    server.shutdown().await;
}
