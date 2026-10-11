//! A downstream buffered record representation shared by generated,
//! runtime, accepted-bytes and paginated terminals (§9.4 REP-02, §9.5).

use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::ApiResponse;
use concord::api;
use concord::codec::{Bytes, BytesMut};
use concord::endpoint::{AcceptedBytesRequest, EndpointId, RuntimeEndpoint};
use concord::error::ErrorCategory;
use concord::http::Method;
use concord::pagination::{CollectPrefix, StopReason};
use concord::records::{
    RecordDecoder, RecordError, RecordErrorKind, decode_complete_records, validate_record_decoder,
};
use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord_test_support::server::{ResponseScript, ScriptedServer};

use crate::support::{OffsetController, loopback_proxy};

static VALIDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
static START_CALLS: AtomicUsize = AtomicUsize::new(0);
static DECODE_CALLS: AtomicUsize = AtomicUsize::new(0);
static TEST_LOCK: Mutex<()> = Mutex::new(());

pub struct MyDecoder;

impl RecordDecoder for MyDecoder {
    type Record = u32;

    const ACCEPT: Option<&'static str> = Some("application/x-concord-u32-records");

    fn validate(head: &concord::response::ResponseHead<'_>) -> Result<(), RecordError> {
        VALIDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        if head
            .headers
            .get("content-type")
            .is_some_and(|value| value == "application/x-concord-u32-records")
        {
            Ok(())
        } else {
            Err(RecordError::new(RecordErrorKind::InvalidMediaType))
        }
    }

    fn start(_head: &concord::response::ResponseHead<'_>) -> Result<Self, RecordError> {
        START_CALLS.fetch_add(1, Ordering::SeqCst);
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        DECODE_CALLS.fetch_add(1, Ordering::SeqCst);
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let record = buffer.split_to(end + 1);
        parse_record(&record[..end]).map(Some)
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Self::Record>, RecordError> {
        if buffer.is_empty() {
            Ok(None)
        } else {
            let record = buffer.split_to(buffer.len());
            parse_record(&record).map(Some)
        }
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(16).expect("positive generated decoder staging bound")
    }
}

fn parse_record(bytes: &[u8]) -> Result<u32, RecordError> {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| RecordError::new(RecordErrorKind::Decode))
}

pub struct MyRecords;

impl ResponseRepresentation<Vec<u32>> for MyRecords {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = <MyDecoder as RecordDecoder>::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<MyDecoder>(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<u32>, ResponseBodyError> {
        decode_complete_records::<MyDecoder>(body, head)
    }
}

api! {
    client BufferedRecordApi {
        base "http://buffered-record.example.test/v1/";
        timeout 30s;

        GET buffered_custom() -> Vec<u32> via MyRecords { expose accepted_bytes; }

        GET pages(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via MyRecords {
            path "pages";
            paginate via OffsetController {
                init OffsetController { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
    }
}

fn build_client(server: &ScriptedServer) -> BufferedRecordApi {
    let proxy = loopback_proxy(server);
    BufferedRecordApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .expect("generated buffered-record client")
}

/// REP-02: page eligibility follows `Driver = Buffered` plus a compatible
/// controller; there is no separate page-response marker trait.
#[test]
fn generated_buffered_record_terminals_have_buffered_and_page_capabilities() {
    let client = BufferedRecordApi::new().expect("side-effect-free buffered-record client");
    let _: AcceptedBytesRequest = client.buffered_custom().accepted_bytes();
    let _: CollectPrefix<u32> = client
        .pages(0, NonZeroU32::new(2).expect("positive page size"))
        .collect_prefix(4);

    fn assert_spec<S>()
    where
        S: ResponseRepresentation<Vec<u32>, Driver = concord::response::Buffered>,
    {
    }
    assert_spec::<MyRecords>();
    let contract = BufferedRecordApi::contract();
    let pages = contract
        .endpoint("buffered_record_api.pages")
        .expect("pages endpoint");
    assert!(pages.terminals().pagination());
    assert!(!pages.terminals().accepted_bytes());
    assert_eq!(
        pages.pagination().expect("pagination").controller(),
        "OffsetController"
    );
    assert!(
        !contract
            .endpoint("buffered_record_api.buffered_custom")
            .expect("buffered endpoint")
            .terminals()
            .pagination()
    );
}

#[tokio::test(flavor = "current_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "process-global decoder probes require whole-test serialization"
)]
async fn generated_runtime_raw_and_paginated_buffered_records_share_the_public_adapter() {
    let _guard = TEST_LOCK.lock().expect("buffered-record test lock");
    VALIDATE_CALLS.store(0, Ordering::SeqCst);
    START_CALLS.store(0, Ordering::SeqCst);
    DECODE_CALLS.store(0, Ordering::SeqCst);

    let server = ScriptedServer::start([
        ResponseScript::ok(b"1\n2\n3".to_vec())
            .with_header("content-type", "application/x-concord-u32-records"),
        ResponseScript::ok(b"raw-decoder-bypass".to_vec())
            .with_header("content-type", "text/plain"),
        ResponseScript::ok(b"4\n5".to_vec())
            .with_header("content-type", "application/x-concord-u32-records"),
        ResponseScript::ok(b"6\n7\n".to_vec())
            .with_header("content-type", "application/x-concord-u32-records"),
        ResponseScript::ok(b"8\n".to_vec())
            .with_header("content-type", "application/x-concord-u32-records"),
    ])
    .expect("buffered-record generated/runtime server");
    let client = build_client(&server);

    let generated: ApiResponse<Vec<u32>> = client
        .buffered_custom()
        .response()
        .await
        .expect("generated buffered records");
    assert_eq!(generated.value(), &[1, 2, 3]);
    assert_eq!(
        generated.metadata().endpoint(),
        "buffered_record_api.buffered_custom"
    );
    assert_eq!(VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(START_CALLS.load(Ordering::SeqCst), 1);
    let decode_after_generated = DECODE_CALLS.load(Ordering::SeqCst);

    let raw = client
        .buffered_custom()
        .accepted_bytes()
        .await
        .expect("raw response bytes bypass records");
    assert_eq!(raw.value(), b"raw-decoder-bypass".as_slice());
    assert_eq!(VALIDATE_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(START_CALLS.load(Ordering::SeqCst), 1);
    assert_eq!(DECODE_CALLS.load(Ordering::SeqCst), decode_after_generated);

    let endpoint = RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.buffered_records").expect("runtime endpoint ID"),
        Method::GET,
    )
    .response::<MyRecords, Vec<u32>>()
    .build()
    .expect("runtime buffered-record endpoint");
    let runtime = client
        .bind(endpoint)
        .expect("runtime buffered-record binding")
        .await
        .expect("runtime buffered records");
    assert_eq!(runtime, [4, 5]);
    assert_eq!(VALIDATE_CALLS.load(Ordering::SeqCst), 2);
    assert_eq!(START_CALLS.load(Ordering::SeqCst), 2);

    let pages = client
        .pages(0, NonZeroU32::new(2).expect("positive page size"))
        .collect_prefix(4)
        .await
        .expect("paginated buffered records");
    assert_eq!(pages.items(), [6, 7, 8]);
    assert_eq!(pages.summary().stop_reason(), StopReason::Exhausted);
    assert_eq!(VALIDATE_CALLS.load(Ordering::SeqCst), 4);
    assert_eq!(START_CALLS.load(Ordering::SeqCst), 4);

    let requests = server.requests();
    assert_eq!(requests.len(), 5);
    assert_eq!(
        requests[0].header("accept"),
        Some("application/x-concord-u32-records")
    );
    assert_eq!(
        requests[1].header("accept"),
        Some("application/x-concord-u32-records")
    );
    assert!(requests[3].target().contains("offset=0&limit=2"));
    assert!(requests[4].target().contains("offset=2&limit=2"));

    let bad_server = ScriptedServer::start([ResponseScript::ok(b"not-a-number".to_vec())
        .with_header("content-type", "application/x-concord-u32-records")])
    .expect("buffered-record decoding error server");
    let error = build_client(&bad_server)
        .buffered_custom()
        .await
        .expect_err("generated buffered decoder failure");
    assert_eq!(error.category(), ErrorCategory::ResponseDecoding);
    assert!(!format!("{error} {error:?}").contains("not-a-number"));
}
