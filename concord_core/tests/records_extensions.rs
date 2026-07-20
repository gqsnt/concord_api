#![cfg(feature = "records")]

use std::error::Error as _;
use std::future::poll_fn;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use concord_core::advanced::{
    BytesMut, HeaderValue, RecordDecoder, RecordError, RecordErrorKind, ResponseHead, SafeProxy,
};
use concord_core::prelude::{ApiResponse, RecordStream, Records};
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream as FuturesStream;

pub struct PipeDecoder;

static PIPE_VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static PIPE_START_CALLS: AtomicUsize = AtomicUsize::new(0);

impl RecordDecoder for PipeDecoder {
    type Record = String;

    fn accept() -> Option<&'static str> {
        Some("application/x-concord-pipe-records")
    }

    fn validate(head: &ResponseHead<'_>) -> Result<(), RecordError> {
        PIPE_VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/x-concord-pipe-records")
        {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
        PIPE_START_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        let Some(end) = buffer.iter().position(|byte| *byte == b'|') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        String::from_utf8(record.to_vec())
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            })
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        if buffer.is_empty() {
            return Ok(None);
        }
        let record = buffer.split_to(buffer.len());
        String::from_utf8(record.to_vec())
            .map(Some)
            .map_err(|error| {
                drop(error);
                RecordError::new(RecordErrorKind::Decode)
            })
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(24).expect("the custom decoder staging bound is positive")
    }
}

api! {
    client CustomRecordsApi {
        base "http://records-extension.example.test/";
        GET Custom() -> concord_core::records::Records<crate::PipeDecoder>;
    }
}

#[cfg(feature = "csv")]
#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
pub struct CsvRow {
    name: String,
    note: String,
    count: u32,
}

#[cfg(feature = "csv")]
api! {
    client CsvRecordsApi {
        base "http://csv-records.example.test/";
        GET Rows() -> concord_core::records::Records<concord_core::records::Csv<crate::CsvRow>>;
    }
}

#[cfg(feature = "ndjson")]
#[derive(Debug, Eq, PartialEq, serde::Deserialize)]
pub struct JsonRow {
    id: u32,
    text: String,
}

#[cfg(feature = "ndjson")]
api! {
    client NdJsonRecordsApi {
        base "http://ndjson-records.example.test/";
        GET Values() -> concord_core::records::Records<concord_core::records::NdJson<crate::JsonRow>>;
    }
}

fn proxy(server: &ScriptedServer) -> SafeProxy {
    SafeProxy::parse(server.url("")).expect("loopback records proxy")
}

fn custom_client(server: &ScriptedServer) -> CustomRecordsApi {
    let proxy = proxy(server);
    CustomRecordsApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("custom records client")
}

async fn next<S>(stream: &mut S) -> Option<S::Item>
where
    S: FuturesStream + Unpin,
{
    poll_fn(|context| Pin::new(&mut *stream).poll_next(context)).await
}

fn chunked(content_type: &'static str, chunks: Vec<Vec<u8>>) -> ResponseScript {
    ResponseScript::Chunked {
        status: 200,
        headers: vec![("content-type".into(), content_type.into())],
        chunks,
    }
}

#[cfg(any(feature = "csv", feature = "ndjson"))]
fn one_byte_chunks(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes.iter().map(|byte| vec![*byte]).collect()
}

#[tokio::test]
async fn downstream_decoder_streams_and_batches_through_generated_public_contracts() {
    PIPE_VALIDATE_CALLS.store(0, Ordering::SeqCst);
    PIPE_START_CALLS.store(0, Ordering::SeqCst);
    let server = ScriptedServer::start([
        chunked(
            "application/x-concord-pipe-records",
            vec![b"alpha|be".to_vec(), b"ta|gamma".to_vec()],
        ),
        chunked(
            "application/x-concord-pipe-records",
            vec![b"first|second|third|".to_vec()],
        ),
        chunked(
            "application/x-concord-pipe-records",
            vec![b"first-record-above-custom-bound-sentinel|".to_vec()],
        ),
        ResponseScript::TruncatedWithHeaders {
            status: 200,
            declared_length: 80,
            prefix: b"record-7c9a31ef".to_vec(),
            headers: vec![(
                "content-type".into(),
                "application/x-concord-pipe-records".into(),
            )],
        },
        chunked(
            "application/x-concord-pipe-records",
            vec![b"unpolled-drop-sentinel|".to_vec()],
        ),
    ])
    .expect("custom records server");
    let client = custom_client(&server);

    let response: ApiResponse<RecordStream<PipeDecoder>> = client
        .custom()
        .response()
        .await
        .expect("custom records metadata response");
    assert_eq!(response.status(), concord_core::advanced::StatusCode::OK);
    assert_eq!(PIPE_VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(PIPE_START_CALLS.load(Ordering::SeqCst), 1);
    let mut records = response.into_value();
    assert_eq!(next(&mut records).await.unwrap().unwrap(), "alpha");
    assert_eq!(next(&mut records).await.unwrap().unwrap(), "beta");
    assert_eq!(next(&mut records).await.unwrap().unwrap(), "gamma");
    assert!(next(&mut records).await.is_none());

    let mut batches = client
        .custom()
        .await
        .expect("custom record stream")
        .batches(NonZeroUsize::new(2).expect("positive batch size"));
    assert_eq!(
        next(&mut batches).await.unwrap().unwrap(),
        ["first".to_owned(), "second".to_owned()]
    );
    assert_eq!(
        next(&mut batches).await.unwrap().unwrap(),
        ["third".to_owned()]
    );
    assert!(next(&mut batches).await.is_none());

    let mut oversized = client.custom().await.expect("bounded record stream");
    let error = next(&mut oversized)
        .await
        .unwrap()
        .expect_err("custom staging excess");
    assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
    assert!(!format!("{error} {error:?}").contains("custom-bound-sentinel"));

    let mut truncated = client
        .custom()
        .await
        .expect("truncated record stream opens");
    let error = next(&mut truncated)
        .await
        .unwrap()
        .expect_err("body read error is a record stream item");
    assert_eq!(error.kind(), RecordErrorKind::Body);
    assert!(error.source().is_none());
    assert!(!format!("{error} {error:?}").contains("record-7c9a31ef"));

    let unpolled = client.custom().await.expect("unpolled record stream opens");
    assert!(!format!("{unpolled:?}").contains("unpolled-drop-sentinel"));
    drop(unpolled);

    server
        .wait_for_requests(5, Duration::from_secs(2))
        .expect("all custom record requests");
    for request in server.requests() {
        assert_eq!(
            request.header("accept"),
            Some("application/x-concord-pipe-records")
        );
    }
}

#[cfg(feature = "csv")]
#[tokio::test]
async fn csv_records_are_typed_chunk_independent_bounded_and_safe() {
    let csv = b"name,note,count\r\n\nalpha,\"comma, doubled \"\"quote\"\" and\nnewline\",1\r\nbeta,final,2";
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let drop_gate = concord_test_support::server::BodyGate::closed();
    let server = ScriptedServer::start([
        chunked("text/csv; charset=utf-8", one_byte_chunks(csv)),
        chunked("text/csv", vec![b"name,note,count\n".to_vec(), oversized]),
        chunked(
            "text/csv",
            vec![b"name,note,count\na,first,1\nb,second,2\nc,third,3\n".to_vec()],
        ),
        ResponseScript::GatedBodyWithHeaders {
            status: 200,
            headers: vec![("content-type".into(), "text/csv".into())],
            body: b"name,note,count\nunpolled-csv-sentinel,value,1\n".to_vec(),
            gate: drop_gate.clone(),
        },
        ResponseScript::TruncatedWithHeaders {
            status: 200,
            declared_length: 100,
            prefix: b"name,note,count\nbody-read-csv-sentinel".to_vec(),
            headers: vec![("content-type".into(), "text/csv".into())],
        },
    ])
    .expect("CSV records server");
    let proxy = proxy(&server);
    let client = CsvRecordsApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("CSV records client");

    let response: ApiResponse<RecordStream<concord_core::records::Csv<CsvRow>>> = client
        .rows()
        .response()
        .await
        .expect("CSV metadata response");
    let mut rows = response.into_value();
    assert_eq!(
        next(&mut rows).await.unwrap().unwrap(),
        CsvRow {
            name: "alpha".into(),
            note: "comma, doubled \"quote\" and\nnewline".into(),
            count: 1,
        }
    );
    assert_eq!(
        next(&mut rows).await.unwrap().unwrap(),
        CsvRow {
            name: "beta".into(),
            note: "final".into(),
            count: 2,
        }
    );
    assert!(next(&mut rows).await.is_none());

    let mut rows = client.rows().await.expect("oversized CSV stream opens");
    let error = next(&mut rows)
        .await
        .unwrap()
        .expect_err("oversized CSV record");
    assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);

    let mut batches = client
        .rows()
        .await
        .expect("CSV batch stream")
        .batches(NonZeroUsize::new(2).expect("positive CSV batch"));
    let first = next(&mut batches).await.unwrap().unwrap();
    assert_eq!(
        first
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    let final_batch = next(&mut batches).await.unwrap().unwrap();
    assert_eq!(
        final_batch
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["c"]
    );
    assert!(next(&mut batches).await.is_none());

    let unpolled = client.rows().await.expect("unpolled CSV stream opens");
    assert!(!format!("{unpolled:?}").contains("unpolled-csv-sentinel"));
    drop(unpolled);
    drop_gate.release();

    let mut rows = client.rows().await.expect("truncated CSV stream opens");
    let error = next(&mut rows)
        .await
        .unwrap()
        .expect_err("CSV body failure");
    assert_eq!(error.kind(), RecordErrorKind::Body);
    assert!(!format!("{error} {error:?}").contains("body-read-csv-sentinel"));

    server
        .wait_for_requests(5, Duration::from_secs(2))
        .expect("five CSV requests");
    assert_eq!(server.requests()[0].header("accept"), Some("text/csv"));
}

#[cfg(feature = "ndjson")]
#[tokio::test]
async fn ndjson_records_are_typed_chunk_independent_bounded_and_safe() {
    let ndjson = b" \r\n{\"id\":1,\"text\":\"alpha\"}\n{\"id\":2,\"text\":\"final\"}";
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    let drop_gate = concord_test_support::server::BodyGate::closed();
    let server = ScriptedServer::start([
        chunked("application/ndjson; charset=utf-8", one_byte_chunks(ndjson)),
        chunked("application/x-ndjson", vec![oversized]),
        chunked(
            "application/x-ndjson",
            vec![b"{\"id\":1,\"text\":\"a\"}\n{\"id\":2,\"text\":\"b\"}\n{\"id\":3,\"text\":\"c\"}\n".to_vec()],
        ),
        ResponseScript::GatedBodyWithHeaders {
            status: 200,
            headers: vec![(
                "content-type".into(),
                "application/x-ndjson".into(),
            )],
            body: b"{\"id\":4,\"text\":\"unpolled-json-sentinel\"}\n".to_vec(),
            gate: drop_gate.clone(),
        },
        ResponseScript::TruncatedWithHeaders {
            status: 200,
            declared_length: 100,
            prefix: b"{\"id\":3,\"text\":\"body-read-json-sentinel\"".to_vec(),
            headers: vec![("content-type".into(), "application/x-ndjson".into())],
        },
    ])
    .expect("NDJSON records server");
    let proxy = proxy(&server);
    let client = NdJsonRecordsApi::builder()
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("NDJSON records client");

    let response: ApiResponse<RecordStream<concord_core::records::NdJson<JsonRow>>> = client
        .values()
        .response()
        .await
        .expect("NDJSON metadata response");
    let mut values = response.into_value();
    assert_eq!(
        next(&mut values).await.unwrap().unwrap(),
        JsonRow {
            id: 1,
            text: "alpha".into(),
        }
    );
    assert_eq!(
        next(&mut values).await.unwrap().unwrap(),
        JsonRow {
            id: 2,
            text: "final".into(),
        }
    );
    assert!(next(&mut values).await.is_none());

    let mut values = client
        .values()
        .await
        .expect("oversized NDJSON stream opens");
    let error = next(&mut values)
        .await
        .unwrap()
        .expect_err("oversized NDJSON value");
    assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);

    let mut batches = client
        .values()
        .await
        .expect("NDJSON batch stream")
        .batches(NonZeroUsize::new(2).expect("positive NDJSON batch"));
    let first = next(&mut batches).await.unwrap().unwrap();
    assert_eq!(
        first.iter().map(|value| value.id).collect::<Vec<_>>(),
        [1, 2]
    );
    let final_batch = next(&mut batches).await.unwrap().unwrap();
    assert_eq!(
        final_batch.iter().map(|value| value.id).collect::<Vec<_>>(),
        [3]
    );
    assert!(next(&mut batches).await.is_none());

    let unpolled = client.values().await.expect("unpolled NDJSON stream opens");
    assert!(!format!("{unpolled:?}").contains("unpolled-json-sentinel"));
    drop(unpolled);
    drop_gate.release();

    let mut values = client
        .values()
        .await
        .expect("truncated NDJSON stream opens");
    let error = next(&mut values)
        .await
        .unwrap()
        .expect_err("NDJSON body failure");
    assert_eq!(error.kind(), RecordErrorKind::Body);
    assert!(!format!("{error} {error:?}").contains("body-read-json-sentinel"));

    server
        .wait_for_requests(5, Duration::from_secs(2))
        .expect("five NDJSON requests");
    assert_eq!(
        server.requests()[0].header("accept"),
        Some("application/x-ndjson")
    );
}

#[test]
fn records_public_markers_are_type_shaped_without_runtime_construction() {
    let _: Option<Records<PipeDecoder>> = None;
    let _: HeaderValue = HeaderValue::from_static("application/x-concord-pipe-records");
}
