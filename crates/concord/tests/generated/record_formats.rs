use std::num::NonZeroU32;
use std::pin::Pin;

use concord::api;
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::error::ErrorCategory;
use concord::http::{Method, SafeProxy};
#[cfg(feature = "csv")]
use concord::records::Csv;
#[cfg(feature = "ndjson")]
use concord::records::NdJson;
use concord::records::{RecordDecoder, RecordError, RecordErrorKind, RecordStream, Records};
use concord::response::{Buffered, PageResponseSpec, ResponseSpec};
use concord_test_support::server::{ResponseScript, ScriptedServer};
use futures_core::Stream;
use serde::Deserialize;

async fn collect<D>(mut stream: RecordStream<D>) -> Result<Vec<D::Record>, RecordError>
where
    D: RecordDecoder,
{
    let mut records = Vec::new();
    while let Some(record) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        records.push(record?);
    }
    Ok(records)
}

async fn stream_error<D>(mut stream: RecordStream<D>) -> RecordError
where
    D: RecordDecoder,
{
    while let Some(record) =
        std::future::poll_fn(|context| Pin::new(&mut stream).poll_next(context)).await
    {
        if let Err(error) = record {
            return error;
        }
    }
    panic!("record stream completed without the expected error");
}

fn chunked(content_type: &str, chunks: Vec<Vec<u8>>) -> ResponseScript {
    ResponseScript::Chunked {
        status: 200,
        headers: vec![("content-type".to_owned(), content_type.to_owned())],
        chunks,
    }
}

fn one_byte(input: &[u8]) -> Vec<Vec<u8>> {
    input.iter().map(|byte| vec![*byte]).collect()
}

#[cfg(feature = "csv")]
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Row {
    name: String,
    count: u32,
}

#[cfg(feature = "csv")]
api! {
    client CsvResponseApi {
        timeout off;
        base "http://csv-response.example.test/v1/";


    GET csv_rows() -> Vec<Row> using Csv<Row> { expose accepted_bytes; }
    GET csv_stream() -> ::concord::records::RecordStream<Csv<Row>> using Records<Csv<Row>>;

    GET csv_pages(offset: u64, limit: std::num::NonZeroU32) -> Vec<Row> using Csv<Row> {
        path ["pages"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate with crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

#[cfg(feature = "csv")]
fn csv_client(server: &ScriptedServer) -> CsvResponseApi {
    let proxy = SafeProxy::parse(server.url("")).expect("CSV loopback proxy");
    CsvResponseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("CSV response client")
}

#[cfg(feature = "csv")]
#[test]
fn csv_is_both_a_complete_page_spec_and_a_record_decoder() {
    fn assert_complete<S>()
    where
        S: ResponseSpec<Output = Vec<Row>, Mode = Buffered> + PageResponseSpec,
    {
    }
    fn assert_decoder<D: RecordDecoder<Record = Row>>() {}

    assert_complete::<Csv<Row>>();
    assert_decoder::<Csv<Row>>();
}

#[cfg(feature = "csv")]
#[tokio::test]
async fn direct_streamed_runtime_raw_and_paginated_csv_have_one_engine_semantics() {
    const BODY: &[u8] = b"name,count\nalpha,1\n\"quoted\nname\",2\n\"comma, name\",3\nfinal,4";
    let expected = [
        Row {
            name: "alpha".to_owned(),
            count: 1,
        },
        Row {
            name: "quoted\nname".to_owned(),
            count: 2,
        },
        Row {
            name: "comma, name".to_owned(),
            count: 3,
        },
        Row {
            name: "final".to_owned(),
            count: 4,
        },
    ];
    let mut large = String::from("name,count\n");
    for index in 0..64_u32 {
        large.push_str(&format!("row-{index},{index}\n"));
    }
    let server = ScriptedServer::start([
        ResponseScript::ok(BODY.to_vec()).with_header("content-type", "text/csv"),
        chunked("text/csv; charset=utf-8", one_byte(BODY)),
        chunked(
            "text/csv",
            vec![
                b"name,count\nalpha".to_vec(),
                b",1\n\"quoted\n".to_vec(),
                b"name\",2\n\"comma, name\",3\nfinal,4".to_vec(),
            ],
        ),
        ResponseScript::ok(large.as_bytes().to_vec()).with_header("content-type", "text/csv"),
        ResponseScript::ok(large.as_bytes().to_vec()).with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\nruntime,9".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"raw-csv-decoder-bypass".to_vec())
            .with_header("content-type", "application/octet-stream"),
        ResponseScript::ok(b"name,count\npage-a,10\npage-b,11\n".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\npage-c,12\n".to_vec())
            .with_header("content-type", "text/csv"),
    ])
    .expect("CSV delivery server");
    let client = csv_client(&server);

    let direct = client.csv_rows().await.expect("direct buffered CSV");
    assert_eq!(direct, expected);
    let one_byte_direct = client
        .csv_rows()
        .await
        .expect("one-byte direct buffered CSV");
    assert_eq!(one_byte_direct, expected);
    let streamed = collect(client.csv_stream().await.expect("CSV stream opens"))
        .await
        .expect("CSV stream decodes");
    assert_eq!(streamed, expected);

    let large_rows = client.csv_rows().await.expect("large source CSV chunk");
    assert_eq!(large_rows.len(), 64);
    assert_eq!(large_rows[0].name, "row-0");
    assert_eq!(large_rows[63].count, 63);
    let large_streamed = collect(client.csv_stream().await.expect("large CSV stream opens"))
        .await
        .expect("large CSV stream decodes");
    assert_eq!(large_streamed, large_rows);

    let runtime = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.csv_rows").expect("runtime CSV endpoint ID"),
        Method::GET,
    )
    .response::<Csv<Row>>()
    .build()
    .expect("runtime CSV response");
    assert_eq!(
        client.request(runtime).await.expect("runtime CSV"),
        [Row {
            name: "runtime".to_owned(),
            count: 9
        }]
    );

    let raw = client
        .csv_rows()
        .accepted_bytes()
        .await
        .expect("CSV response bytes bypass");
    assert_eq!(raw.value(), b"raw-csv-decoder-bypass".as_slice());

    let pages = client
        .csv_pages(0, NonZeroU32::new(2).expect("positive CSV page size"))
        .paginate(4_u32)
        .await
        .expect("paginated CSV");
    assert_eq!(
        pages,
        [
            Row {
                name: "page-a".to_owned(),
                count: 10,
            },
            Row {
                name: "page-b".to_owned(),
                count: 11,
            },
            Row {
                name: "page-c".to_owned(),
                count: 12,
            },
        ]
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert!(
        requests
            .iter()
            .all(|request| request.header("accept") == Some("text/csv"))
    );
    assert!(requests[7].target().contains("offset=0&limit=2"));
    assert!(requests[8].target().contains("offset=2&limit=2"));
}

#[cfg(feature = "csv")]
#[tokio::test]
async fn direct_and_streamed_csv_failures_match_categories_and_redaction() {
    let oversized = format!("name,count\n{},1", "x".repeat(1024 * 1024 + 1));
    let server = ScriptedServer::start([
        ResponseScript::ok(b"media-csv-secret".to_vec())
            .with_header("content-type", "application/json"),
        ResponseScript::ok(b"media-csv-secret".to_vec())
            .with_header("content-type", "application/json"),
        ResponseScript::ok(b"name,count\n\"closed\"after,1\n".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\n\"closed\"after,1\n".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\nprivate,not-a-number\n".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\nprivate,not-a-number\n".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\n\"incomplete-csv-secret".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(b"name,count\n\"incomplete-csv-secret".to_vec())
            .with_header("content-type", "text/csv"),
        ResponseScript::ok(oversized.as_bytes().to_vec()).with_header("content-type", "text/csv"),
        ResponseScript::ok(oversized.as_bytes().to_vec()).with_header("content-type", "text/csv"),
    ])
    .expect("CSV failure server");
    let client = csv_client(&server);

    let direct_media = client
        .csv_rows()
        .await
        .expect_err("direct CSV media mismatch");
    assert_eq!(direct_media.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{direct_media} {direct_media:?}").contains("media-csv-secret"));
    let streamed_media = client
        .csv_stream()
        .await
        .expect_err("streamed CSV media mismatch");
    assert_eq!(streamed_media.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{streamed_media} {streamed_media:?}").contains("media-csv-secret"));

    let direct_framing = client.csv_rows().await.expect_err("direct CSV framing");
    assert_eq!(direct_framing.category(), ErrorCategory::ResponseDecoding);
    let streamed_framing =
        stream_error(client.csv_stream().await.expect("framing stream opens")).await;
    assert_eq!(streamed_framing.kind(), RecordErrorKind::InvalidFraming);

    let direct_decode = client.csv_rows().await.expect_err("direct CSV decode");
    assert_eq!(direct_decode.category(), ErrorCategory::ResponseDecoding);
    let streamed_decode =
        stream_error(client.csv_stream().await.expect("decode stream opens")).await;
    assert_eq!(streamed_decode.kind(), RecordErrorKind::Decode);

    let direct_incomplete = client.csv_rows().await.expect_err("direct CSV incomplete");
    assert_eq!(
        direct_incomplete.category(),
        ErrorCategory::ResponseDecoding
    );
    let streamed_incomplete =
        stream_error(client.csv_stream().await.expect("incomplete stream opens")).await;
    assert_eq!(
        streamed_incomplete.kind(),
        RecordErrorKind::IncompleteRecord
    );

    let direct_limit = client.csv_rows().await.expect_err("direct CSV staging");
    assert_eq!(direct_limit.category(), ErrorCategory::ResponseDecoding);
    let streamed_limit = stream_error(client.csv_stream().await.expect("limit stream opens")).await;
    assert_eq!(streamed_limit.kind(), RecordErrorKind::LimitExceeded);

    let automatic = format!(
        "{direct_framing:?} {direct_decode:?} {direct_incomplete:?} {direct_limit:?} \
         {streamed_framing:?} {streamed_decode:?} {streamed_incomplete:?} {streamed_limit:?}"
    );
    for forbidden in [
        "closed",
        "not-a-number",
        "incomplete-csv-secret",
        &"x".repeat(64),
    ] {
        assert!(!automatic.contains(forbidden));
    }
}

#[cfg(feature = "ndjson")]
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct Event {
    id: u32,
}

#[cfg(feature = "ndjson")]
api! {
    client NdJsonResponseApi {
        timeout off;
        base "http://ndjson-response.example.test/v1/";


    GET ndjson_events() -> Vec<Event> using NdJson<Event> { expose accepted_bytes; }
    GET ndjson_stream() -> ::concord::records::RecordStream<NdJson<Event>> using Records<NdJson<Event>>;

    GET ndjson_pages(offset: u64, limit: std::num::NonZeroU32) -> Vec<Event> using NdJson<Event> {
        path ["pages"];
        query {
            "offset" = endpoint.offset;
            "limit" = endpoint.limit;
        }
        paginate with crate::support::OffsetController {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
}}

#[cfg(feature = "ndjson")]
fn ndjson_client(server: &ScriptedServer) -> NdJsonResponseApi {
    let proxy = SafeProxy::parse(server.url("")).expect("NDJSON loopback proxy");
    NdJsonResponseApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("NDJSON response client")
}

#[cfg(feature = "ndjson")]
#[test]
fn ndjson_is_both_a_complete_page_spec_and_a_record_decoder() {
    fn assert_complete<S>()
    where
        S: ResponseSpec<Output = Vec<Event>, Mode = Buffered> + PageResponseSpec,
    {
    }
    fn assert_decoder<D: RecordDecoder<Record = Event>>() {}

    assert_complete::<NdJson<Event>>();
    assert_decoder::<NdJson<Event>>();
}

#[cfg(feature = "ndjson")]
#[tokio::test]
async fn direct_streamed_runtime_raw_and_paginated_ndjson_have_one_engine_semantics() {
    const BODY: &[u8] = b"{\"id\":1}\n \r\n{\"id\":2}\r\n{\"id\":3}";
    let expected = [Event { id: 1 }, Event { id: 2 }, Event { id: 3 }];
    let mut large = String::new();
    for index in 0..64_u32 {
        large.push_str(&format!("{{\"id\":{index}}}\n"));
    }
    let server = ScriptedServer::start([
        ResponseScript::ok(BODY.to_vec()).with_header("content-type", "application/x-ndjson"),
        chunked("application/ndjson; charset=utf-8", one_byte(BODY)),
        chunked(
            "application/x-ndjson",
            vec![
                b"{\"id\":1}\n {".to_vec(),
                b"\"id\":2}\r".to_vec(),
                b"\n{\"id\":3}".to_vec(),
            ],
        ),
        ResponseScript::ok(large.as_bytes().to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(large.as_bytes().to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"{\"id\":9}".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"raw-ndjson-decoder-bypass".to_vec())
            .with_header("content-type", "text/plain"),
        ResponseScript::ok(b"{\"id\":10}\n{\"id\":11}\n".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"{\"id\":12}\n".to_vec())
            .with_header("content-type", "application/x-ndjson"),
    ])
    .expect("NDJSON delivery server");
    let client = ndjson_client(&server);

    let direct = client
        .ndjson_events()
        .await
        .expect("direct buffered NDJSON");
    assert_eq!(direct, expected);
    let one_byte_direct = client
        .ndjson_events()
        .await
        .expect("one-byte direct buffered NDJSON");
    assert_eq!(one_byte_direct, expected);
    let streamed = collect(client.ndjson_stream().await.expect("NDJSON stream opens"))
        .await
        .expect("NDJSON stream decodes");
    assert_eq!(streamed, expected);

    let large_events = client
        .ndjson_events()
        .await
        .expect("large source NDJSON chunk");
    assert_eq!(large_events.len(), 64);
    assert_eq!(large_events[0].id, 0);
    assert_eq!(large_events[63].id, 63);
    let large_streamed = collect(
        client
            .ndjson_stream()
            .await
            .expect("large NDJSON stream opens"),
    )
    .await
    .expect("large NDJSON stream decodes");
    assert_eq!(large_streamed, large_events);

    let runtime = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.ndjson_events").expect("runtime NDJSON endpoint ID"),
        Method::GET,
    )
    .response::<NdJson<Event>>()
    .build()
    .expect("runtime NDJSON response");
    assert_eq!(
        client.request(runtime).await.expect("runtime NDJSON"),
        [Event { id: 9 }]
    );

    let raw = client
        .ndjson_events()
        .accepted_bytes()
        .await
        .expect("NDJSON response bytes bypass");
    assert_eq!(raw.value(), b"raw-ndjson-decoder-bypass".as_slice());

    let pages = client
        .ndjson_pages(0, NonZeroU32::new(2).expect("positive NDJSON page size"))
        .paginate(4_u32)
        .await
        .expect("paginated NDJSON");
    assert_eq!(
        pages,
        [Event { id: 10 }, Event { id: 11 }, Event { id: 12 }]
    );

    let requests = server.requests();
    assert_eq!(requests.len(), 9);
    assert!(
        requests
            .iter()
            .all(|request| { request.header("accept") == Some("application/x-ndjson") })
    );
    assert!(requests[7].target().contains("offset=0&limit=2"));
    assert!(requests[8].target().contains("offset=2&limit=2"));
}

#[cfg(feature = "ndjson")]
#[tokio::test]
async fn direct_and_streamed_ndjson_failures_match_categories_and_redaction() {
    let oversized = "x".repeat(1024 * 1024 + 1);
    let server = ScriptedServer::start([
        ResponseScript::ok(b"media-ndjson-secret".to_vec())
            .with_header("content-type", "application/json"),
        ResponseScript::ok(b"media-ndjson-secret".to_vec())
            .with_header("content-type", "application/json"),
        ResponseScript::ok(b"{\"id\":broken-ndjson-secret}\n".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"{\"id\":broken-ndjson-secret}\n".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"{\"id\":13".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(b"{\"id\":13".to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(oversized.as_bytes().to_vec())
            .with_header("content-type", "application/x-ndjson"),
        ResponseScript::ok(oversized.as_bytes().to_vec())
            .with_header("content-type", "application/x-ndjson"),
    ])
    .expect("NDJSON failure server");
    let client = ndjson_client(&server);

    let direct_media = client
        .ndjson_events()
        .await
        .expect_err("direct NDJSON media mismatch");
    assert_eq!(direct_media.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{direct_media} {direct_media:?}").contains("media-ndjson-secret"));
    let streamed_media = client
        .ndjson_stream()
        .await
        .expect_err("streamed NDJSON media mismatch");
    assert_eq!(streamed_media.category(), ErrorCategory::ResponseContract);
    assert!(!format!("{streamed_media} {streamed_media:?}").contains("media-ndjson-secret"));

    let direct_decode = client
        .ndjson_events()
        .await
        .expect_err("direct NDJSON decode");
    assert_eq!(direct_decode.category(), ErrorCategory::ResponseDecoding);
    let streamed_decode =
        stream_error(client.ndjson_stream().await.expect("decode stream opens")).await;
    assert_eq!(streamed_decode.kind(), RecordErrorKind::Decode);

    let direct_incomplete = client
        .ndjson_events()
        .await
        .expect_err("direct NDJSON incomplete");
    assert_eq!(
        direct_incomplete.category(),
        ErrorCategory::ResponseDecoding
    );
    let streamed_incomplete = stream_error(
        client
            .ndjson_stream()
            .await
            .expect("incomplete stream opens"),
    )
    .await;
    assert_eq!(
        streamed_incomplete.kind(),
        RecordErrorKind::IncompleteRecord
    );

    let direct_limit = client
        .ndjson_events()
        .await
        .expect_err("direct NDJSON staging");
    assert_eq!(direct_limit.category(), ErrorCategory::ResponseDecoding);
    let streamed_limit =
        stream_error(client.ndjson_stream().await.expect("limit stream opens")).await;
    assert_eq!(streamed_limit.kind(), RecordErrorKind::LimitExceeded);

    let automatic = format!(
        "{direct_decode:?} {direct_incomplete:?} {direct_limit:?} \
         {streamed_decode:?} {streamed_incomplete:?} {streamed_limit:?}"
    );
    for forbidden in ["broken-ndjson-secret", &"x".repeat(64)] {
        assert!(!automatic.contains(forbidden));
    }
}
