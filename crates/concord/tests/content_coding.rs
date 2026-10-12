//! Supported content codings use decoded budgets over verified native TLS.
#![cfg(feature = "ndjson")]
use concord_core::__private::{
    AcceptedStatus, ClientCore, ControllerCursor, ControllerInit, Effect, EmptyBody, EndpointSpec,
    HeadBuilder, PageableEndpoint, PreparedHead, RequestMeta, ResponseRepresentation,
    build_client_core, collect_prefix_request, default_http_config, default_runtime_config,
    request, response_request,
};
use concord_core::contracts::{
    ApiError, BodyDisposition, DispatchKnowledge, ErrorCategory, FailureStage, ItemStream, Method,
    RuntimeConfigBuilder, StreamErrorKind, StreamState,
};
use concord_core::pagination::{PaginationErrorKind, StopReason};
use concord_core::retry::{RetryDecision, RetryPolicy};
use concord_extras::{
    pagination::{OffsetInit, OffsetPagination},
    records::NdJson,
};
use concord_test_support::tls::TlsServer;
use serde::Deserialize;
use std::{
    marker::PhantomData,
    num::NonZeroU32,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Row {
    id: u32,
}
struct Endpoint<O>(PhantomData<fn() -> O>);
impl<O: Send + 'static> EndpointSpec for Endpoint<O>
where
    NdJson: ResponseRepresentation<O>,
{
    type Input = ();
    type Body = EmptyBody;
    type Output = O;
    type Response = NdJson;
    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_contract(
            "s3.compression",
            Method::GET,
            Some(Duration::from_secs(5)),
            AcceptedStatus::Success,
            Effect::Safe,
        )
    }
    fn prepare_head(&self, mut b: HeadBuilder) -> Result<PreparedHead, ApiError> {
        b.push_static("records")?;
        b.finish()
    }
    fn into_body(self) {}
}
#[derive(Clone)]
struct PageEndpoint {
    offset: u64,
}
impl EndpointSpec for PageEndpoint {
    type Input = ();
    type Body = EmptyBody;
    type Output = Vec<Row>;
    type Response = NdJson;
    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_contract(
            "s3.compression_pages",
            Method::GET,
            Some(Duration::from_secs(5)),
            AcceptedStatus::Success,
            Effect::Safe,
        )
    }
    fn prepare_head(&self, mut b: HeadBuilder) -> Result<PreparedHead, ApiError> {
        b.push_static("pages")?;
        b.push_pagination_query_value("offset", &self.offset)?;
        b.finish()
    }
    fn into_body(self) {}
}
impl PageableEndpoint for PageEndpoint {
    type Controller = OffsetPagination;
    fn controller_init(&self) -> ControllerInit<Self> {
        OffsetInit {
            offset: self.offset,
            page_size: NonZeroU32::new(2).unwrap(),
        }
    }
    fn apply_cursor(&mut self, cursor: ControllerCursor<Self>) {
        self.offset = cursor;
    }
    fn page_clone(&self) -> Self {
        self.clone()
    }
    fn page_body_present(&self) -> bool {
        false
    }
}
#[derive(Deserialize)]
struct Corpus {
    decoded: usize,
    frame: usize,
    gzip: Vec<u8>,
    br: Vec<u8>,
    deflate: Vec<u8>,
}
fn corpus() -> Corpus {
    serde_json::from_str(include_str!("content_coding_fixture.json"))
        .expect("fixed bounded fixture JSON")
}
fn core(server: &TlsServer, runtime: RuntimeConfigBuilder, disabled: bool) -> ClientCore {
    let mut http = default_http_config()
        .http1_only()
        .add_root_certificate_pem(server.certificate_pem());
    if disabled {
        http = http.disable_gzip().disable_brotli().disable_deflate();
    }
    build_client_core(&server.url("localhost", "/"), http, runtime)
        .expect("verified TLS client setup")
}
fn queue(server: &TlsServer, encoding: &str, bytes: &[u8]) {
    assert!(bytes.len() <= 65);
    server
        .queue_response_with_headers(
            200,
            &[
                ("Content-Type", "application/x-ndjson"),
                ("Content-Encoding", encoding),
            ],
            bytes.to_vec(),
        )
        .expect("fixed compressed wire fixture");
}
fn runtime(classifications: &Arc<AtomicUsize>) -> RuntimeConfigBuilder {
    let called = classifications.clone();
    let policy = RetryPolicy::builder()
        .max_retries_per_operation(2)
        .without_budget()
        .classify_fn(move |_| {
            called.fetch_add(1, Ordering::SeqCst);
            RetryDecision::Retry
        })
        .build()
        .expect("bounded retry appetite");
    default_runtime_config().retry_policy(policy)
}
fn check(error: &ApiError, kind: StreamErrorKind, category: ErrorCategory) {
    assert_eq!(error.category(), category);
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.stream_summary().unwrap().error_kind(), Some(kind));
    assert_eq!(
        error.last_response_status(),
        Some(concord_core::contracts::StatusCode::OK)
    );
}

#[tokio::test]
async fn content_coding_stream_decoded_limits_and_no_replay() {
    let data = corpus();
    let mut cases = 0;
    for (encoding, bytes) in [
        ("gzip", &data.gzip),
        ("br", &data.br),
        ("deflate", &data.deflate),
    ] {
        for mode in 0..5 {
            let server = TlsServer::start().expect("TLS server");
            let classifications = Arc::new(AtomicUsize::new(0));
            let encoded = if mode == 4 {
                &bytes[..6]
            } else {
                bytes.as_slice()
            };
            queue(&server, encoding, encoded);
            let mut config = runtime(&classifications);
            config = match mode {
                1 => config.stream_response_limit(data.decoded - 1),
                2 => config.stream_frame_limit(data.frame - 1),
                3 => config.stream_max_records(1),
                _ => config.stream_response_limit(data.decoded),
            };
            let client = core(&server, config, false);
            let response =
                response_request(request(&client, Endpoint::<ItemStream<Row>>(PhantomData)))
                    .await
                    .expect("compressed accepted head opens stream");
            assert_eq!(response.body_disposition(), BodyDisposition::StreamOpened);
            assert_eq!(response.metadata().attempts_started(), 1);
            assert_eq!(response.metadata().policy_retries(), 0);
            assert!(response.headers().get("content-length").is_none());
            assert!(response.headers().get("content-encoding").is_none());
            let mut stream = response.into_value();
            let mut rows = Vec::new();
            let mut failure = None;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(r) => rows.push(r),
                    Err(e) => {
                        failure = Some(e);
                        break;
                    }
                }
            }
            if mode == 0 {
                assert!(failure.is_none());
                assert_eq!(rows, [Row { id: 1 }, Row { id: 2 }]);
                assert_eq!(stream.summary().state(), StreamState::Eof);
                assert_eq!(stream.summary().acquired_bytes(), data.decoded as u64);
            } else {
                let expected = [
                    StreamErrorKind::ByteLimit,
                    StreamErrorKind::FrameLimit,
                    StreamErrorKind::RecordLimit,
                    StreamErrorKind::Body,
                ][mode - 1];
                check(
                    &failure.expect("specific decompressed limit/read failure"),
                    expected,
                    ErrorCategory::ResponseBody,
                );
                assert_eq!(stream.summary().state(), StreamState::Failed);
                if mode == 2 {
                    assert_eq!(stream.summary().delivered_records(), 0);
                }
                if mode == 3 {
                    assert_eq!(stream.summary().delivered_records(), 1);
                }
                if mode == 1 {
                    assert!(stream.summary().acquired_bytes() > bytes.len() as u64);
                }
            }
            assert!(stream.next().await.is_none());
            let summary = stream.finish();
            assert_eq!(server.requests().len(), 1);
            assert_eq!(classifications.load(Ordering::SeqCst), 0);
            println!(
                "CONTENT_CODING encoding={encoding} mode={mode} wire={} acquired={} records={} state={:?} requests=1 classifier_calls=0",
                encoded.len(),
                summary.acquired_bytes(),
                summary.delivered_records(),
                summary.state()
            );
            cases += 1;
        }
    }
    println!(
        "CONTENT_CODING stream_cases={cases} seed=fixed-enumeration maximum_decoded=2092 maximum_encoded=65"
    );
}

#[tokio::test]
async fn content_coding_buffered_decoded_limit_and_disablement() {
    let data = corpus();
    let mut cases = 0;
    for (encoding, bytes) in [
        ("gzip", &data.gzip),
        ("br", &data.br),
        ("deflate", &data.deflate),
    ] {
        for mode in 0..3 {
            let server = TlsServer::start().expect("TLS server");
            queue(&server, encoding, bytes);
            let classifications = Arc::new(AtomicUsize::new(0));
            let limit = if mode == 1 {
                data.decoded - 1
            } else {
                data.decoded
            };
            let client = core(
                &server,
                runtime(&classifications).response_body_limit(limit),
                mode == 2,
            );
            let result =
                response_request(request(&client, Endpoint::<Vec<Row>>(PhantomData))).await;
            if mode == 0 {
                let response = result.expect("decoded exact body limit");
                assert_eq!(response.value(), &[Row { id: 1 }, Row { id: 2 }]);
                assert_eq!(response.body_disposition(), BodyDisposition::Complete);
                assert!(response.headers().get("content-length").is_none());
            } else {
                let error =
                    result.expect_err("decoded first excess / disabled undecoded representation");
                assert_eq!(
                    error.category(),
                    if mode == 1 {
                        ErrorCategory::ResponseBody
                    } else {
                        ErrorCategory::ResponseDecoding
                    }
                );
                assert_eq!(
                    error.stage(),
                    if mode == 1 {
                        FailureStage::ResponseAcquisition
                    } else {
                        FailureStage::ResponseConversion
                    }
                );
                assert_eq!(
                    error.dispatch_knowledge(),
                    DispatchKnowledge::ResponseObserved
                );
                assert_eq!(
                    error.last_response_status(),
                    Some(concord_core::contracts::StatusCode::OK)
                );
            }
            assert_eq!(server.requests().len(), 1);
            assert_eq!(classifications.load(Ordering::SeqCst), 0);
            cases += 1;
        }
    }
    println!(
        "CONTENT_CODING buffered_cases={cases} positive_exact_limit=true disabled_raw_conversion_control=true"
    );
}

#[tokio::test]
async fn content_coding_traversal_charges_decoded_aggregate_bytes() {
    let data = corpus();
    let mut cases = 0;
    for (encoding, bytes) in [
        ("gzip", &data.gzip),
        ("br", &data.br),
        ("deflate", &data.deflate),
    ] {
        for ceiling in [2 * data.decoded - 1, 2 * data.decoded] {
            let server = TlsServer::start().expect("TLS server");
            queue(&server, encoding, bytes);
            queue(&server, encoding, bytes);
            let classifications = Arc::new(AtomicUsize::new(0));
            let client = core(
                &server,
                runtime(&classifications).pagination_total_response_body_limit(ceiling),
                false,
            );
            let result =
                collect_prefix_request(request(&client, PageEndpoint { offset: 0 }), 2).await;
            let summary = if ceiling == 2 * data.decoded {
                let prefix = result.expect("exact aggregate decoded total");
                assert_eq!(
                    prefix.items(),
                    &[Row { id: 1 }, Row { id: 2 }, Row { id: 1 }, Row { id: 2 }]
                );
                assert_eq!(prefix.summary().stop_reason(), StopReason::PageBound);
                assert_eq!(prefix.summary().pages_received(), 2);
                prefix.summary().clone()
            } else {
                let error = result.expect_err("second page decoded aggregate excess");
                assert_eq!(
                    error.pagination_kind(),
                    Some(PaginationErrorKind::CollectedResponseByteCeilingExceeded)
                );
                assert_eq!(error.stage(), FailureStage::PaginationDecision);
                assert_eq!(
                    error.dispatch_knowledge(),
                    DispatchKnowledge::ResponseObserved
                );
                let summary = error.traversal_summary().unwrap();
                assert_eq!(summary.pages_received(), 1);
                assert_eq!(summary.pages_delivered(), 1);
                assert_eq!(summary.items_delivered(), 2);
                assert_eq!(summary.stop_reason(), StopReason::Failed);
                summary.clone()
            };
            assert_eq!(summary.acquired_response_bytes(), (2 * data.decoded) as u64);
            assert_eq!(summary.application_attempts(), 2);
            assert_eq!(summary.automatic_replays(), 0);
            assert_eq!(server.requests().len(), 2);
            assert_eq!(classifications.load(Ordering::SeqCst), 0);
            cases += 1;
        }
    }
    println!(
        "CONTENT_CODING traversal_cases={cases} exact_decoded_total=4184 pages_at_most=2 one_send_per_page=true"
    );
}

#[tokio::test]
async fn content_coding_case_variants_preserve_supported_decoding() {
    // RFC 9110 8.4.1: content codings are case-insensitive. Compare exact same
    // known-positive compressed bytes; only the Content-Encoding token case changes.
    let data = corpus();
    let mut failures = Vec::new();
    for (lower, mixed, bytes) in [
        ("gzip", "GZip", &data.gzip),
        ("br", "BR", &data.br),
        ("deflate", "Deflate", &data.deflate),
    ] {
        let server = TlsServer::start().expect("TLS setup");
        let classifications = Arc::new(AtomicUsize::new(0));
        let client = core(&server, runtime(&classifications), false);
        queue(&server, lower, bytes);
        queue(&server, mixed, bytes);
        let positive = response_request(request(&client, Endpoint::<Vec<Row>>(PhantomData)))
            .await
            .expect("same bytes with lowercase coding decode");
        assert_eq!(positive.value(), &[Row { id: 1 }, Row { id: 2 }]);
        match response_request(request(&client, Endpoint::<Vec<Row>>(PhantomData))).await {
            Ok(response) => assert_eq!(response.value(), positive.value()),
            Err(error) => {
                assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
                assert_eq!(error.stage(), FailureStage::ResponseConversion);
                assert_eq!(
                    error.dispatch_knowledge(),
                    DispatchKnowledge::ResponseObserved
                );
                assert_eq!(
                    error.last_response_status(),
                    Some(concord_core::contracts::StatusCode::OK)
                );
                println!(
                    "CONTENT_CODING lower={lower} positive=2_records mixed={mixed} actual={:?}/{:?} status=200 dispatch={:?} sends=1 classifier_calls=0",
                    error.category(),
                    error.stage(),
                    error.dispatch_knowledge()
                );
                failures.push(mixed);
            }
        }
        assert_eq!(server.requests().len(), 2);
        assert_eq!(classifications.load(Ordering::SeqCst), 0);
    }
    assert!(
        failures.is_empty(),
        "case-insensitive content codings unexpectedly failed: {failures:?}"
    );
}
