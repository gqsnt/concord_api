//! Offline wire, failure, resource-bound and redaction checks.
mod common;

use common::*;
use concord_sdk_arxiv::concord::{ErrorKind, bytes::Bytes};
use concord_sdk_arxiv::{
    Arxiv, ArxivConfig, BODY_LIMIT, ERROR_BODY_LIMIT, Error, IdList, PageSize, Policy, SearchTerm,
    http_client,
};
use concord_testserver::{Body, Recorded, Reply, TestServer};
use std::{
    future::Future,
    pin::Pin,
    time::{Duration, Instant},
};

async fn search_error(reply: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Error {
    let server = TestServer::start(reply).await;
    let result = client(&server)
        .papers()
        .search(SearchTerm::new("private-sentinel").unwrap())
        .send()
        .await
        .unwrap_err();
    assert_eq!(server.requests().len(), 1, "failures never trigger a retry");
    server.shutdown().await;
    result
}
fn kind(error: &Error) -> ErrorKind {
    match error {
        Error::Concord(e) => e.kind(),
        e => panic!("expected Concord failure: {e:?}"),
    }
}

#[test]
fn invalid_base_is_a_redacted_build_error_without_network_io() {
    let error = Arxiv::with_http(http_client().unwrap())
        .with_base_url("https://user:private-sentinel@localhost/api")
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let error = Error::from(error);
    assert!(!format!("{error:?} {error}").contains("private-sentinel"));
}

#[tokio::test]
async fn wire_shapes_encoding_accept_no_auth_no_body_and_shared_pacing() {
    Arxiv::validate_header_names().unwrap();
    let server = TestServer::start(|request: &Recorded| -> Reply {
        assert_eq!(request.method, "GET");
        assert_eq!(request.path(), "/api/query");
        assert_eq!(request.header("accept"), Some("application/atom+xml"));
        assert!(request.header("authorization").is_none());
        assert!(request.header("content-type").is_none());
        assert!(request.body.is_empty());
        Reply::text(
            200,
            "application/atom+xml",
            if request.query_all("id_list").is_empty() {
                EMPTY
            } else {
                IDS
            },
        )
    })
    .await;
    let clock = VirtualClock::default();
    let api = Arxiv::new(
        http_client().unwrap(),
        ArxivConfig {
            policy: Policy::with_clock(clock.clone()),
        },
    )
    .with_base_url(format!("{}/api", server.url()))
    .unwrap();
    api.papers()
        .search(SearchTerm::new("électron & x+y").unwrap())
        .send()
        .await
        .unwrap();
    api.clone()
        .papers()
        .fetch(IdList::new(&["2601.00001v2", "hep-ex/0307015v1"]).unwrap())
        .send()
        .await
        .unwrap();
    let requests = server.requests();
    assert_eq!(
        requests[0].target,
        "/api/query?search_query=all%3A%22%C3%A9lectron+%26+x%2By%22&start=0&max_results=10"
    );
    assert_eq!(
        requests[1].target,
        "/api/query?id_list=2601.00001v2%2Chep-ex%2F0307015v1&start=0&max_results=50"
    );
    assert_eq!(clock.waits(), [Duration::from_secs(3)]);
    server.shutdown().await;
}

#[tokio::test]
async fn all_documented_provider_error_classes_are_atom_errors_even_under_http_200() {
    // Manual sections 3.1.1.2 and 3.4 define these independent classes.
    for document in [
        include_str!("../fixtures/error-start-type.xml"),
        include_str!("../fixtures/error-start-negative.xml"),
        include_str!("../fixtures/error-max-type.xml"),
        include_str!("../fixtures/error-max-negative.xml"),
        ERROR,
        include_str!("../fixtures/error-legacy-id.xml"),
        include_str!("../fixtures/error-max-large.xml"),
    ] {
        let error = search_error(move |_: &Recorded| -> Reply {
            Reply::text(200, "application/atom+xml", document)
        })
        .await;
        let Error::Api(failure) = error else {
            panic!("{error:?}")
        };
        assert_eq!(failure.meta.status().as_u16(), 200);
        assert!(
            failure
                .problem
                .id
                .starts_with("http://arxiv.org/api/errors#")
        );
        assert!(!failure.problem.summary.is_empty());
    }
}

#[tokio::test]
async fn non_success_status_is_authoritative_and_no_redirect_or_retry_occurs() {
    for status in [302, 400, 404, 429, 503] {
        let error = search_error(move |_: &Recorded| -> Reply {
            Reply::text(status, "application/atom+xml", ERROR)
                .header("location", "/should-not-follow")
        })
        .await;
        let Error::Http(failure) = error else {
            panic!("{error:?}")
        };
        assert_eq!(failure.meta.status().as_u16(), status);
        assert!(failure.document.unwrap().problem.is_some());
    }
}

#[tokio::test]
async fn wrong_missing_repeated_and_malformed_media_are_metadata_failures_before_body_read() {
    for media in [
        None,
        Some("text/html"),
        Some("application/xml"),
        Some("application/atom+xml; charset=iso-8859-1"),
        Some("not a media type"),
    ] {
        let error = search_error(move |_: &Recorded| -> Reply {
            let reply = Reply::new(200).with_body(Body::Stall);
            match media {
                Some(media) => reply.header("content-type", media),
                None => reply,
            }
        })
        .await;
        assert_eq!(kind(&error), ErrorKind::Metadata);
    }
    let error = search_error(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", EMPTY)
            .header("content-type", "application/atom+xml")
    })
    .await;
    assert_eq!(kind(&error), ErrorKind::Metadata);
}

#[tokio::test]
async fn malformed_truncated_non_atom_and_invalid_utf8_bodies_are_decode_failures() {
    for body in [
        include_bytes!("../fixtures/malformed.xml").as_slice(),
        include_bytes!("../fixtures/truncated.xml").as_slice(),
        b"<feed/>".as_slice(),
        b"\xff".as_slice(),
        b"".as_slice(),
        b"<!DOCTYPE feed [<!ENTITY secret SYSTEM 'file:///never-read'>]><feed/>".as_slice(),
    ] {
        let error = search_error(move |_: &Recorded| -> Reply {
            Reply::text(200, "application/atom+xml", body)
        })
        .await;
        assert_eq!(kind(&error), ErrorKind::Decode);
    }
}

#[tokio::test]
async fn required_fields_counts_duplicates_and_node_limits_fail_explicitly() {
    let cases = [
        FIRST.replace("<title>Synthetic electron &amp; photon study</title>", ""),
        FIRST.replace(
            "<o:totalResults>2</o:totalResults>",
            "<o:totalResults>+2</o:totalResults>",
        ),
        FIRST.replace("<o:startIndex>0</o:startIndex>", ""),
        FIRST.replace(
            "<o:itemsPerPage>1</o:itemsPerPage>",
            "<o:itemsPerPage>18446744073709551616</o:itemsPerPage>",
        ),
        FIRST.replace(
            "<title>Synthetic electron &amp; photon study</title>",
            "<title>A</title><title>B</title>",
        ),
        format!(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\">{}</feed>",
            "<unknown/>".repeat(50_001)
        ),
    ];
    for body in cases {
        let error = search_error(move |_: &Recorded| -> Reply {
            Reply::text(200, "application/atom+xml", &body)
        })
        .await;
        assert_eq!(kind(&error), ErrorKind::Decode);
    }
}

#[tokio::test]
async fn declared_success_limit_covers_fixed_and_chunked_bodies_and_native_prepared_send() {
    for chunked in [false, true] {
        let error = search_error(move |_: &Recorded| -> Reply {
            let reply = Reply::new(200).header("content-type", "application/atom+xml");
            if chunked {
                reply.with_body(Body::Chunks(vec![(
                    Duration::ZERO,
                    Bytes::from(vec![b' '; BODY_LIMIT + 1]),
                )]))
            } else {
                reply.body(vec![b' '; BODY_LIMIT + 1])
            }
        })
        .await;
        assert_eq!(kind(&error), ErrorKind::Limit);
    }
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", vec![b' '; BODY_LIMIT + 1])
    })
    .await;
    let error = client(&server)
        .papers()
        .search(SearchTerm::new("x").unwrap())
        .prepare()
        .unwrap()
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    server.shutdown().await;
}

#[tokio::test]
async fn exact_byte_limit_and_case_insensitive_utf8_media_are_accepted() {
    let mut body = EMPTY.to_owned();
    body.extend(std::iter::repeat_n(' ', BODY_LIMIT - body.len()));
    let server = TestServer::start(move |_: &Recorded| -> Reply {
        Reply::text(200, "Application/Atom+XML; charset=\"UTF-8\"", &body)
    })
    .await;
    assert!(
        client(&server)
            .papers()
            .search(SearchTerm::new("x").unwrap())
            .send()
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn http_error_documents_keep_status_for_every_body_failure_class() {
    for case in 0..5 {
        let error = search_error(move |_: &Recorded| -> Reply {
            match case {
                0 => Reply::text(503, "text/html", "<html>proxy</html>"),
                1 => Reply::text(503, "application/atom+xml", "<feed>"),
                2 => Reply::text(
                    503,
                    "application/atom+xml",
                    vec![b' '; ERROR_BODY_LIMIT + 1],
                ),
                3 => Reply::new(503)
                    .header("content-type", "application/atom+xml")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(b"<feed>")])),
                _ => Reply::text(503, "application/atom+xml", ""),
            }
        })
        .await;
        let Error::Http(failure) = error else {
            panic!("{error:?}")
        };
        assert_eq!(failure.meta.status().as_u16(), 503);
        let document = failure.document.unwrap_err();
        assert_eq!(
            document.kind(),
            [
                ErrorKind::Metadata,
                ErrorKind::Decode,
                ErrorKind::Limit,
                ErrorKind::Transport,
                ErrorKind::Decode
            ][case]
        );
        assert_eq!(document.head().unwrap().status().as_u16(), 503);
    }
}

#[tokio::test]
async fn transport_failures_before_and_after_head_are_distinguishable() {
    let error = search_error(|_: &Recorded| -> Reply { Reply::hang_up() }).await;
    assert_eq!(kind(&error), ErrorKind::Transport);
    let Error::Concord(e) = error else {
        unreachable!()
    };
    assert!(e.head().is_none());
    let error = search_error(|_: &Recorded| -> Reply {
        Reply::new(200)
            .header("content-type", "application/atom+xml")
            .with_body(Body::FailAfter(vec![Bytes::from_static(b"<feed>")]))
    })
    .await;
    assert_eq!(kind(&error), ErrorKind::Transport);
    let Error::Concord(e) = error else {
        unreachable!()
    };
    assert_eq!(e.head().unwrap().status().as_u16(), 200);
}

#[tokio::test]
async fn input_bounds_refuse_before_io_and_unmatched_ids_are_not_invented() {
    for term in ["", " ", "a\nb", "a\"b", "a\\b"] {
        assert!(SearchTerm::new(term).is_err());
    }
    assert!(SearchTerm::new("x".repeat(513)).is_err());
    for ids in [
        vec![],
        vec!["a,b"],
        vec!["https://arxiv.org/abs/1"],
        vec!["a&x=1"],
        vec!["x"; 51],
    ] {
        assert!(IdList::new(&ids).is_err());
    }
    assert!(PageSize::new(0).is_err() && PageSize::new(101).is_err());
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", EMPTY)
    })
    .await;
    let api = client(&server);
    for start in [30_000, 29_999, u32::MAX] {
        assert!(matches!(
            api.papers()
                .search(SearchTerm::new("x").unwrap())
                .set_start(start)
                .send()
                .await,
            Err(Error::Input(_))
        ));
    }
    assert!(server.requests().is_empty());
    // ID grammar is delegated; a syntactically safe but unknown identifier need not yield an entry.
    assert!(
        api.papers()
            .fetch(IdList::new(&["2601.99999"]).unwrap())
            .send()
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    server.shutdown().await;
}

#[tokio::test]
async fn pagination_refuses_wrong_offsets_overfull_and_stalled_pages() {
    for body in [
        SECOND.to_owned(),
        FIRST.replace(
            "<o:itemsPerPage>1</o:itemsPerPage>",
            "<o:itemsPerPage>11</o:itemsPerPage>",
        ),
        EMPTY.replace(
            "<o:totalResults>0</o:totalResults>",
            "<o:totalResults>2</o:totalResults>",
        ),
        FIRST.replace(
            "<o:totalResults>2</o:totalResults>",
            "<o:totalResults>0</o:totalResults>",
        ),
    ] {
        let server = TestServer::start(move |_: &Recorded| -> Reply {
            Reply::text(200, "application/atom+xml", &body)
        })
        .await;
        let mut pages = client(&server)
            .papers()
            .search(SearchTerm::new("x").unwrap())
            .pages(
                concord_sdk_arxiv::concord_pagination::Resume::Start,
                std::num::NonZeroUsize::new(2).unwrap(),
            );
        assert!(matches!(
            pages.next_page().await.unwrap_err().provider_error(),
            Some(Error::Pagination(_))
        ));
        assert!(pages.next_page().await.is_err());
        assert_eq!(server.requests().len(), 1);
        server.shutdown().await;
    }
}

#[tokio::test]
async fn debug_and_display_redact_request_text_provider_messages_cookies_and_parser_input() {
    let secret = "private-sentinel";
    let server = TestServer::start(move |_: &Recorded| -> Reply {
        Reply::text(
            200,
            "application/atom+xml",
            ERROR.replace("incorrect id format for 1234.12345", secret),
        )
        .header("set-cookie", secret)
    })
    .await;
    let api = client(&server);
    let term = SearchTerm::new(secret).unwrap();
    assert!(!format!("{term:?}").contains(secret));
    let ids = IdList::new(&[secret]).unwrap();
    assert!(!format!("{ids:?}").contains(secret));
    let request = api.papers().search(term);
    for shown in [
        format!("{api:?}"),
        format!("{:?}", api.papers()),
        format!("{request:?}"),
        format!("{:?}", request.clone().prepare().unwrap()),
        format!("{:?}", api.papers().fetch(ids)),
    ] {
        assert!(!shown.contains(secret));
    }
    let error = request.send().await.unwrap_err();
    assert!(!format!("{error:?} {error}").contains(secret));
    let Error::Api(failure) = error else {
        unreachable!()
    };
    assert_eq!(failure.problem.summary, secret);
    assert_eq!(failure.meta.headers()["set-cookie"], secret);
    server.shutdown().await;
    for status in [200, 400] {
        let error = search_error(move |_: &Recorded| -> Reply {
            Reply::text(status, "application/atom+xml", "<private-sentinel>")
                .header("set-cookie", "private-sentinel")
        })
        .await;
        assert!(!format!("{error:?} {error}").contains(secret));
        assert!(std::error::Error::source(&error).is_none());
    }
}

struct OverflowClock;
impl concord_ratelimit::Clock for OverflowClock {
    fn now(&self) -> Duration {
        Duration::MAX
    }
    fn sleep(&self, _: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async {})
    }
}
#[tokio::test]
async fn admission_overflow_is_reported_before_dispatch() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", EMPTY)
    })
    .await;
    let api = Arxiv::new(
        http_client().unwrap(),
        ArxivConfig {
            policy: Policy::with_clock(OverflowClock),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    assert!(matches!(
        api.papers()
            .search(SearchTerm::new("x").unwrap())
            .send()
            .await,
        Err(Error::Admission(concord_ratelimit::Error::Overflow))
    ));
    assert!(server.requests().is_empty());
    server.shutdown().await;
}

#[tokio::test]
async fn real_clock_delays_a_second_dispatch_through_a_clone() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        Reply::text(200, "application/atom+xml", EMPTY)
    })
    .await;
    let api = Arxiv::with_http(http_client().unwrap())
        .with_base_url(server.url())
        .unwrap();
    let started = Instant::now();
    api.papers()
        .search(SearchTerm::new("one").unwrap())
        .send()
        .await
        .unwrap();
    api.clone()
        .papers()
        .search(SearchTerm::new("two").unwrap())
        .send()
        .await
        .unwrap();
    assert!(started.elapsed() >= Duration::from_secs(3));
    assert_eq!(server.requests().len(), 2);
    server.shutdown().await;
}
