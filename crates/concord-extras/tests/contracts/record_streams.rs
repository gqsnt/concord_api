//! Loopback evidence for managed record streams owned by extras codecs:
//! final NDJSON/CSV tails, malformed records as one terminal correlated
//! error, frame and record limits without unbounded buffering (V3-P11), and
//! acquisition that is not end of body (V3-P09).

use std::marker::PhantomData;
use std::time::Duration;

use concord_core::__private::{
    AcceptedStatus, ClientCore, Effect, EmptyBody, EndpointSpec, HeadBuilder, PreparedHead,
    RequestMeta, ResponseRepresentation, build_client_core, default_http_config,
    default_runtime_config, request, response_request,
};
use concord_core::contracts::{
    ApiError, BodyDisposition, DispatchKnowledge, ErrorCategory, FailureStage, ItemStream, Method,
    RuntimeConfigBuilder, StatusCode, StreamErrorKind, StreamState, StreamSummary,
};
use concord_test_support::server::{BodyGate, ResponseScript, ScriptedServer};
use serde::Deserialize;

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Row {
    id: u32,
}

struct StreamEndpoint<R> {
    marker: PhantomData<fn() -> R>,
}

impl<R> StreamEndpoint<R> {
    fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<R> EndpointSpec for StreamEndpoint<R>
where
    R: ResponseRepresentation<ItemStream<Row>>,
{
    type Input = ();
    type Body = EmptyBody;
    type Output = ItemStream<Row>;
    type Response = R;

    fn request_meta(&self) -> RequestMeta {
        RequestMeta::new_with_contract(
            "record_streams.export",
            Method::GET,
            Some(Duration::from_secs(5)),
            AcceptedStatus::Success,
            Effect::Safe,
        )
    }

    fn prepare_head(&self, mut builder: HeadBuilder) -> Result<PreparedHead, ApiError> {
        builder.push_static("export")?;
        builder.finish()
    }

    fn into_body(self) {}
}

fn core(server: &ScriptedServer, runtime: RuntimeConfigBuilder) -> ClientCore {
    build_client_core(&server.url("/v1/"), default_http_config(), runtime)
        .expect("loopback client core")
}

fn chunked(content_type: &str, chunks: &[&[u8]]) -> ResponseScript {
    ResponseScript::Chunked {
        status: 200,
        headers: vec![("Content-Type".to_owned(), content_type.to_owned())],
        chunks: chunks.iter().map(|chunk| chunk.to_vec()).collect(),
    }
}

async fn open<R>(core: &ClientCore) -> ItemStream<Row>
where
    R: ResponseRepresentation<ItemStream<Row>>,
{
    let response = response_request(request(core, StreamEndpoint::<R>::new()))
        .await
        .expect("the accepted head opens the record stream");
    assert_eq!(response.body_disposition(), BodyDisposition::StreamOpened);
    let stream = response.into_value();
    assert_eq!(stream.summary().state(), StreamState::Active);
    stream
}

fn kind(error: &ApiError) -> Option<StreamErrorKind> {
    error.stream_summary().and_then(StreamSummary::error_kind)
}

/// Real-clock pending-read idle budget of the overdue-read cases: exceeded on
/// purpose, and far longer than loopback delivery of a released chunk.
const SHORT_IDLE: Duration = Duration::from_millis(300);

/// Sends the head and `first` at once; `rest` and the end of the body only
/// after `gate` is released.
fn gated(content_type: &str, first: &[u8], rest: &[u8], gate: &BodyGate) -> ResponseScript {
    ResponseScript::GatedChunkedWithHeaders {
        status: 200,
        headers: vec![("Content-Type".to_owned(), content_type.to_owned())],
        first_chunk: first.to_vec(),
        remaining_chunk: rest.to_vec(),
        gate: gate.clone(),
    }
}

/// Drops one `next` future while its underlying read is pending, keeping the
/// stream handle (and its idle baseline).
async fn abandon_pending_read(stream: &mut ItemStream<Row>) {
    assert!(
        tokio::time::timeout(Duration::from_millis(50), stream.next())
            .await
            .is_err(),
        "the next record needs a pending read"
    );
}

/// Makes the withheld tail available only after the idle budget has been
/// spent, then lets the loopback transport deliver it before the next poll.
async fn release_after_idle_expired(gate: &BodyGate) {
    tokio::time::sleep(SHORT_IDLE * 2).await;
    gate.release();
    tokio::time::sleep(Duration::from_millis(300)).await;
}

/// The overdue read is one correlated idle timeout, the summary retains the
/// failure, the stream then ends, and the request was sent exactly once.
async fn assert_overdue_idle_timeout(
    stream: &mut ItemStream<Row>,
    server: &ScriptedServer,
    delivered: usize,
) {
    let error = stream
        .next()
        .await
        .expect("an idle timeout, not a record or a clean end of body")
        .expect_err("the pending read exceeded the idle budget before data or EOF arrived");
    assert_stream_error(&error, StreamErrorKind::IdleTimeout, ErrorCategory::Timeout);
    assert!(stream.next().await.is_none());
    let summary = stream.summary();
    assert_eq!(summary.state(), StreamState::Failed);
    assert_eq!(summary.error_kind(), Some(StreamErrorKind::IdleTimeout));
    assert_eq!(summary.delivered_records(), delivered);
    assert_eq!(summary.body_disposition(), BodyDisposition::Failed);
    assert_eq!(server.request_count(), 1, "the request is never resent");
}

fn assert_stream_error(error: &ApiError, expected: StreamErrorKind, category: ErrorCategory) {
    assert_eq!(kind(error), Some(expected));
    assert_eq!(error.category(), category);
    assert_eq!(error.stage(), FailureStage::StreamConsumption);
    assert_eq!(
        error.dispatch_knowledge(),
        DispatchKnowledge::ResponseObserved
    );
    assert_eq!(error.last_response_status(), Some(StatusCode::OK));
    assert!(error.operation_id().is_some());
    assert_eq!(error.endpoint(), "record_streams.export");
}

#[cfg(feature = "ndjson")]
mod ndjson {
    use concord_extras::records::NdJson;

    use super::*;

    #[tokio::test]
    async fn v3_p11_final_line_without_newline_is_parsed_once_and_empty_lines_are_ignored() {
        let server = ScriptedServer::start([chunked(
            "application/x-ndjson",
            &[b"{\"id\":1}\n\n  \r\n{\"id\":", b"2}\r\n\n", b"{\"id\":3}"],
        )])
        .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<NdJson>(&core).await;
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row.expect("valid NDJSON record"));
        }
        assert_eq!(rows, [Row { id: 1 }, Row { id: 2 }, Row { id: 3 }]);
        assert!(stream.next().await.is_none());
        let summary = stream.finish();
        assert_eq!(summary.state(), StreamState::Eof);
        assert_eq!(summary.delivered_records(), 3);
        assert_eq!(summary.body_disposition(), BodyDisposition::Complete);
    }

    #[tokio::test]
    async fn v3_p11_malformed_record_is_one_terminal_correlated_error() {
        let server = ScriptedServer::start([chunked(
            "application/x-ndjson",
            &[b"{\"id\":1}\n{\"id\":broken-secret-4411}\n{\"id\":3}\n"],
        )])
        .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        let error = stream.next().await.unwrap().expect_err("malformed record");
        assert_stream_error(
            &error,
            StreamErrorKind::Decode,
            ErrorCategory::ResponseDecoding,
        );
        assert!(!format!("{error} {error:?}").contains("broken-secret-4411"));
        assert!(stream.next().await.is_none());
        let summary = stream.summary();
        assert_eq!(summary.state(), StreamState::Failed);
        assert_eq!(summary.delivered_records(), 1);
    }

    #[tokio::test]
    async fn v3_p11_incomplete_final_record_is_not_silently_dropped() {
        let server =
            ScriptedServer::start([chunked("application/x-ndjson", &[b"{\"id\":1}\n{\"id\":"])])
                .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        let error = stream.next().await.unwrap().expect_err("incomplete tail");
        assert_stream_error(
            &error,
            StreamErrorKind::IncompleteRecord,
            ErrorCategory::ResponseDecoding,
        );
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn v3_p11_record_limit_withholds_the_next_record() {
        let server = ScriptedServer::start([chunked(
            "application/x-ndjson",
            &[b"{\"id\":1}\n{\"id\":2}\n{\"id\":3}\n"],
        )])
        .expect("loopback server");
        let core = core(&server, default_runtime_config().stream_max_records(2));
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 2 });
        let error = stream.next().await.unwrap().expect_err("record limit");
        assert_stream_error(
            &error,
            StreamErrorKind::RecordLimit,
            ErrorCategory::ResponseBody,
        );
        assert!(stream.next().await.is_none());
        assert_eq!(stream.summary().delivered_records(), 2);
    }

    #[tokio::test]
    async fn v3_p09_finish_before_the_final_record_is_an_application_stop() {
        let server = ScriptedServer::start([chunked(
            "application/x-ndjson",
            &[b"{\"id\":1}\n", b"{\"id\":2}\n"],
        )])
        .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        let summary = stream.finish();
        assert_eq!(summary.state(), StreamState::Application);
        assert_eq!(summary.delivered_records(), 1);
        assert_ne!(summary.body_disposition(), BodyDisposition::Complete);
    }

    // Review F02 (STREAM-02/03): a pending read whose idle budget expired
    // before the producer made data or EOF available is an idle timeout.

    #[tokio::test]
    async fn v3_p10_overdue_ndjson_record_data_is_one_idle_timeout_without_resend() {
        let gate = BodyGate::closed();
        let server = ScriptedServer::start([gated(
            "application/x-ndjson",
            b"{\"id\":1}\n{\"id\":",
            b"2}\n",
            &gate,
        )])
        .expect("loopback server");
        let core = core(
            &server,
            default_runtime_config().stream_idle_timeout(SHORT_IDLE),
        );
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        abandon_pending_read(&mut stream).await;
        release_after_idle_expired(&gate).await;
        assert_overdue_idle_timeout(&mut stream, &server, 1).await;
    }

    #[tokio::test]
    async fn v3_p10_overdue_eof_cannot_finalize_an_unterminated_ndjson_line() {
        let gate = BodyGate::closed();
        // The final line has no LF: only the end of the body would parse it.
        let server = ScriptedServer::start([gated(
            "application/x-ndjson",
            b"{\"id\":1}\n{\"id\":2}",
            b"",
            &gate,
        )])
        .expect("loopback server");
        let core = core(
            &server,
            default_runtime_config().stream_idle_timeout(SHORT_IDLE),
        );
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        abandon_pending_read(&mut stream).await;
        release_after_idle_expired(&gate).await;
        assert_overdue_idle_timeout(&mut stream, &server, 1).await;
    }

    #[tokio::test]
    async fn v3_p10_ndjson_record_data_within_the_idle_limit_resumes_an_abandoned_read() {
        let gate = BodyGate::closed();
        let server = ScriptedServer::start([gated(
            "application/x-ndjson",
            b"{\"id\":1}\n{\"id\":",
            b"2}\n{\"id\":3}",
            &gate,
        )])
        .expect("loopback server");
        let core = core(
            &server,
            default_runtime_config().stream_idle_timeout(Duration::from_secs(20)),
        );
        let mut stream = open::<NdJson>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        abandon_pending_read(&mut stream).await;
        gate.release();
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 2 });
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 3 });
        assert!(stream.next().await.is_none());
        let summary = stream.summary();
        assert_eq!(summary.state(), StreamState::Eof);
        assert_eq!(summary.delivered_records(), 3);
        assert_eq!(server.request_count(), 1);
    }
}

#[cfg(feature = "csv")]
mod csv {
    use concord_extras::records::Csv;

    use super::*;

    #[tokio::test]
    async fn v3_p11_csv_final_record_without_newline_is_decoded() {
        let server = ScriptedServer::start([chunked("text/csv", &[b"id\r\n1\r\n", b"\r\n2\n3"])])
            .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<Csv>(&core).await;
        let mut rows = Vec::new();
        while let Some(row) = stream.next().await {
            rows.push(row.expect("valid CSV record"));
        }
        assert_eq!(rows, [Row { id: 1 }, Row { id: 2 }, Row { id: 3 }]);
        assert_eq!(stream.summary().state(), StreamState::Eof);
    }

    #[tokio::test]
    async fn v3_p11_invalid_csv_quoting_fails_the_stream() {
        let server =
            ScriptedServer::start([chunked("text/csv", &[b"id\n1\n2\"quoted-secret-9a1b\n4\n"])])
                .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<Csv>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        let error = stream.next().await.unwrap().expect_err("invalid quoting");
        assert_stream_error(
            &error,
            StreamErrorKind::InvalidFraming,
            ErrorCategory::ResponseDecoding,
        );
        assert!(!format!("{error} {error:?}").contains("quoted-secret-9a1b"));
        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn v3_p11_unterminated_csv_quote_at_end_of_body_is_incomplete() {
        let server = ScriptedServer::start([chunked("text/csv", &[b"id\n1\n\"2"])])
            .expect("loopback server");
        let core = core(&server, default_runtime_config());
        let mut stream = open::<Csv>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        let error = stream
            .next()
            .await
            .unwrap()
            .expect_err("unterminated quote");
        assert_stream_error(
            &error,
            StreamErrorKind::IncompleteRecord,
            ErrorCategory::ResponseDecoding,
        );
    }

    #[tokio::test]
    async fn v3_p11_frame_limit_bounds_an_unterminated_quoted_record() {
        // An open quote never ends the record, so without a frame ceiling the
        // decoder would buffer the whole body.
        let server =
            ScriptedServer::start([chunked("text/csv", &[b"id\n\"", &[b'x'; 64], &[b'y'; 64]])])
                .expect("loopback server");
        let core = core(&server, default_runtime_config().stream_frame_limit(32));
        let mut stream = open::<Csv>(&core).await;
        let error = stream.next().await.unwrap().expect_err("frame limit");
        assert_stream_error(
            &error,
            StreamErrorKind::FrameLimit,
            ErrorCategory::ResponseBody,
        );
        assert!(stream.next().await.is_none());
        assert_eq!(stream.summary().delivered_records(), 0);
    }

    // Review F02 (STREAM-02/03): a pending read whose idle budget expired
    // before the producer made data or EOF available is an idle timeout.

    #[tokio::test]
    async fn v3_p10_overdue_csv_record_data_is_one_idle_timeout_without_resend() {
        let gate = BodyGate::closed();
        let server = ScriptedServer::start([gated("text/csv", b"id\n1\n2", b"\n3\n", &gate)])
            .expect("loopback server");
        let core = core(
            &server,
            default_runtime_config().stream_idle_timeout(SHORT_IDLE),
        );
        let mut stream = open::<Csv>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        abandon_pending_read(&mut stream).await;
        release_after_idle_expired(&gate).await;
        assert_overdue_idle_timeout(&mut stream, &server, 1).await;
    }

    #[tokio::test]
    async fn v3_p10_overdue_eof_cannot_finalize_an_unterminated_csv_record() {
        let gate = BodyGate::closed();
        // The final record has no line terminator: only the end of the body
        // would decode it.
        let server = ScriptedServer::start([gated("text/csv", b"id\n1\n2", b"", &gate)])
            .expect("loopback server");
        let core = core(
            &server,
            default_runtime_config().stream_idle_timeout(SHORT_IDLE),
        );
        let mut stream = open::<Csv>(&core).await;
        assert_eq!(stream.next().await.unwrap().unwrap(), Row { id: 1 });
        abandon_pending_read(&mut stream).await;
        release_after_idle_expired(&gate).await;
        assert_overdue_idle_timeout(&mut stream, &server, 1).await;
    }
}
