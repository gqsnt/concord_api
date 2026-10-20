#![cfg(feature = "ndjson")]

use concord::http::SafeProxy;
use concord::records::NdJson;
use concord::{ItemStream, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client R7Records {
        base "http://records.example.test/";
        POST buffered(body rows: Vec<String> via NdJson) -> Vec<String> via NdJson;
        GET streaming() -> ItemStream<String> via NdJson;
    }
}

fn response(body: Vec<u8>) -> ResponseScript {
    ResponseScript::Complete {
        status: 200,
        headers: vec![("Content-Type".into(), "application/x-ndjson".into())],
        body,
    }
}

fn client(server: &ScriptedServer) -> R7Records {
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    R7Records::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .unwrap()
}

#[tokio::test]
async fn r7_ndjson_exact_payload_roundtrips_through_the_real_buffered_driver() {
    let value = "x".repeat(1_048_574);
    let mut encoded = serde_json::to_vec(&value).unwrap();
    assert_eq!(encoded.len(), 1_048_576);
    encoded.push(b'\n');
    let server = ScriptedServer::start([response(encoded.clone())]).unwrap();
    let result = client(&server).buffered(vec![value.clone()]).await;
    assert_eq!(server.requests()[0].body(), encoded);
    assert_eq!(result.expect("exact payload must roundtrip"), vec![value]);
}

#[tokio::test]
async fn r7_ndjson_exact_payload_streams_with_lf_crlf_and_final_eof() {
    for delimiter in [b"\n".as_slice(), b"\r\n".as_slice(), b"".as_slice()] {
        let value = "x".repeat(1_048_574);
        let mut encoded = serde_json::to_vec(&value).unwrap();
        encoded.extend_from_slice(delimiter);
        let server = ScriptedServer::start([response(encoded)]).unwrap();
        let mut stream = client(&server).streaming().await.unwrap();
        assert_eq!(
            stream.next().await.unwrap().expect("boundary record"),
            value
        );
        let tail = stream.next().await.map(|r| r.map(|value| value.len()));
        assert!(tail.is_none(), "unexpected structural tail: {tail:?}");
        assert_eq!(stream.summary().delivered_records(), 1);
    }
}

#[tokio::test]
async fn r7_ndjson_first_excess_is_rejected_in_requests_buffered_and_streaming_responses() {
    let excess = "x".repeat(1_048_575);
    let mut encoded = serde_json::to_vec(&excess).unwrap();
    encoded.push(b'\n');
    let server = ScriptedServer::start([response(encoded.clone()), response(encoded)]).unwrap();
    let api = client(&server);
    let error = api.buffered(vec![excess]).await.unwrap_err();
    assert_eq!(error.category(), concord::error::ErrorCategory::RequestBody);
    assert_eq!(server.request_count(), 0);
    assert!(api.buffered(vec![]).await.is_err());
    let mut stream = api.streaming().await.unwrap();
    let error = stream
        .next()
        .await
        .unwrap()
        .expect_err("oversized payload is withheld");
    assert_eq!(
        error.stream_summary().unwrap().error_kind(),
        Some(concord::response::StreamErrorKind::FrameLimit)
    );
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn r7_ndjson_one_large_http_chunk_uses_the_same_bounded_driver() {
    let value = "x".repeat(1_048_574);
    let mut body = serde_json::to_vec(&value).unwrap();
    body.push(b'\n');
    let script = ResponseScript::Chunked {
        status: 200,
        headers: vec![("Content-Type".into(), "application/x-ndjson".into())],
        chunks: vec![body],
    };
    let server = ScriptedServer::start([script.clone(), script]).unwrap();
    let api = client(&server);
    assert_eq!(api.buffered(vec![]).await.unwrap(), vec![value.clone()]);
    let mut stream = api.streaming().await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap(), value);
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn r7_ndjson_runtime_frame_caps_count_delimiters_and_handle_one_byte_chunks() {
    for delimiter in [b"\n".as_slice(), b"\r\n".as_slice(), b"".as_slice()] {
        for excess in [false, true] {
            let value = "x".repeat(16 - 2 - delimiter.len() + usize::from(excess));
            let mut body = serde_json::to_vec(&value).unwrap();
            body.extend_from_slice(delimiter);
            let script = ResponseScript::Chunked {
                status: 200,
                headers: vec![("Content-Type".into(), "application/x-ndjson".into())],
                chunks: body.iter().map(|byte| vec![*byte]).collect(),
            };
            let server = ScriptedServer::start([script]).unwrap();
            let proxy = SafeProxy::parse(server.url("")).unwrap();
            let api = R7Records::builder()
                .configure_http(move |http| http.proxy(proxy))
                .configure_runtime(|runtime| runtime.stream_frame_limit(16))
                .build()
                .unwrap();
            let mut stream = api.streaming().await.unwrap();
            let record = stream.next().await.unwrap();
            if excess {
                assert!(record.is_err());
            } else {
                assert_eq!(record.unwrap(), value);
            }
            assert!(stream.next().await.is_none());
        }
    }
}

#[tokio::test]
async fn r7_ndjson_large_split_records_blank_lines_body_record_and_final_record_limits() {
    let value = "x".repeat(1_048_574);
    let mut body = serde_json::to_vec(&value).unwrap();
    body.push(b'\n');
    let script = ResponseScript::Chunked {
        status: 200,
        headers: vec![("Content-Type".into(), "application/x-ndjson".into())],
        chunks: body.chunks(65_536).map(<[u8]>::to_vec).collect(),
    };
    let server = ScriptedServer::start([script]).unwrap();
    let api = client(&server);
    let mut stream = api.streaming().await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap(), value);
    assert!(stream.next().await.is_none());

    let server = ScriptedServer::start([response(b"\n\r\n\"a\"\n\"b\"\n".to_vec())]).unwrap();
    let proxy = SafeProxy::parse(server.url("")).unwrap();
    let api = R7Records::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| runtime.stream_max_records(1))
        .build()
        .unwrap();
    let mut stream = api.streaming().await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap(), "a");
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(
        error.stream_summary().unwrap().error_kind(),
        Some(concord::response::StreamErrorKind::RecordLimit)
    );
    assert_eq!(stream.summary().delivered_records(), 1);
    assert!(stream.next().await.is_none());
    for body in [
        b"private-malformed\n".as_slice(),
        b"\"private-incomplete".as_slice(),
    ] {
        let server = ScriptedServer::start([response(body.to_vec())]).unwrap();
        let api = client(&server);
        let mut stream = api.streaming().await.unwrap();
        let error = stream.next().await.unwrap().unwrap_err();
        assert!(!format!("{error} {error:?}").contains("private-"));
        assert!(stream.next().await.is_none());
    }
    let server = ScriptedServer::start([response(b"\"too much data\"\n".to_vec())]).unwrap();
    let api = client(&server);
    let options =
        concord::CallOptions::new().cap_response_bytes(std::num::NonZeroUsize::new(4).unwrap());
    assert!(
        api.streaming().with_options(&options).await.is_err(),
        "known Content-Length is rejected before stream acquisition"
    );
    let server = ScriptedServer::start([ResponseScript::Chunked {
        status: 200,
        headers: vec![("Content-Type".into(), "application/x-ndjson".into())],
        chunks: vec![b"\"too much data\"\n".to_vec()],
    }])
    .unwrap();
    let api = client(&server);
    let mut stream = api.streaming().with_options(&options).await.unwrap();
    let error = stream.next().await.unwrap().unwrap_err();
    assert_eq!(
        error.stream_summary().unwrap().error_kind(),
        Some(concord::response::StreamErrorKind::ByteLimit)
    );
    assert!(stream.next().await.is_none());
}
