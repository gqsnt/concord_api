#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::future::IntoFuture;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord::__development::{
    BenchmarkOperation, benchmark_public_head, benchmark_ready_credentials,
};
use concord::advanced::{
    BytesMut, CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, Method,
    NoopRateLimiter, PreparedEndpoint, RecordDecoder, RecordError, SafeProxy, StatusCode,
};
use concord::api;
use concord::prelude::{Bytes, PageLimit, StreamBody};
use concord::{PaginatedRequest, Request};
use concord_test_support::server::{ResponseScript, ScriptedServer};

pub struct BenchCodec;

impl EncodeBody for BenchCodec {
    type Input = String;

    const CONTENT_TYPE: Option<&'static str> = Some("application/vnd.concord.benchmark");

    fn encode(input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::from(input))
    }
}

impl DecodeBody for BenchCodec {
    type Output = String;

    const ACCEPT: Option<&'static str> = Some("application/vnd.concord.benchmark");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new("benchmark UTF-8"))
    }
}

pub struct PageCodec;

impl DecodeBody for PageCodec {
    type Output = Vec<u8>;

    const ACCEPT: Option<&'static str> = None;

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        Ok(body.to_vec())
    }
}

pub struct BenchRecords;

impl RecordDecoder for BenchRecords {
    type Record = Bytes;

    fn start(_head: &concord::advanced::ResponseHead<'_>) -> Result<Self, RecordError> {
        Ok(Self)
    }

    fn decode(&mut self, buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        let Some(end) = buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut record = buffer.split_to(end + 1);
        record.truncate(end);
        Ok(Some(record.freeze()))
    }

    fn decode_eof(&mut self, buffer: &mut BytesMut) -> Result<Option<Bytes>, RecordError> {
        Ok((!buffer.is_empty()).then(|| buffer.split_to(buffer.len()).freeze()))
    }

    fn max_buffered_bytes() -> NonZeroUsize {
        NonZeroUsize::new(64 * 1024).expect("positive benchmark staging bound")
    }
}

api! {
    client BenchApi {
        base "http://benchmark.example.test/";
        credentials {
            secret api_secret: String;
            credential application_key = bearer(secret.api_secret);
        }
    }

    scope Records(collection: String) {
        path ["records", scope.collection];
        GET Show(id: String) -> Bytes {
            path [ep.id];
            auth off;
        }
    }
    GET Bounded() -> Bytes {
        auth off;
    }
    GET Protected() -> Bytes {
        auth bearer application_key;
    }
    GET Limited() -> Bytes {
        auth off;
        rate_limit { 1000000 / 1s; }
    }
    GET Pages(offset: u64, limit: std::num::NonZeroU32) -> crate::PageCodec {
        query { "offset" = ep.offset; "limit" = ep.limit; }
        auth off;
        paginate concord::advanced::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
    POST Codec(body: crate::BenchCodec) -> crate::BenchCodec {
        auth off;
    }
    GET Streamed() -> Stream<OctetStream> {
        auth off;
    }
}

api! {
    client CredentialBenchApi {
        base "https://credential-benchmark.example.test/";
        credentials {
            secret api_secret: String;
            credential application_key = bearer(secret.api_secret);
        }
    }

    GET CredentialReady() -> Bytes {
        auth bearer application_key;
    }
}

const EXPECTED_CATEGORIES: [&str; 11] = [
    "core client construction",
    "generated client, facade, and request creation",
    "public head preparation",
    "codec encoding and decoding",
    "body-limit polling",
    "credential ready-path preparation and finalization",
    "limiter ready-path acquisition",
    "pagination loop overhead",
    "stream wrapping",
    "generic specification selection",
    "record framing",
];

#[derive(Default)]
struct CategoryAudit {
    executed: BTreeSet<&'static str>,
}

impl CategoryAudit {
    fn record(&mut self, name: &'static str) {
        assert!(
            EXPECTED_CATEGORIES.contains(&name),
            "unregistered benchmark category {name}"
        );
        assert!(
            self.executed.insert(name),
            "duplicate benchmark category {name}"
        );
    }

    fn finish(self) {
        assert_eq!(
            self.executed.len(),
            EXPECTED_CATEGORIES.len(),
            "not every benchmark category executed"
        );
        for expected in EXPECTED_CATEGORIES {
            assert!(
                self.executed.contains(expected),
                "benchmark category did not execute: {expected}"
            );
        }
        println!("benchmark operation audit: 11/11 named categories executed");
    }
}

fn iterations() -> usize {
    if std::env::args().any(|argument| argument == "--smoke") {
        return 1;
    }
    std::env::var("CONCORD_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 1_000)
}

fn report(audit: &mut CategoryAudit, name: &'static str, iterations: usize, started: Instant) {
    let elapsed = started.elapsed();
    audit.record(name);
    println!("{name}: {iterations} iteration(s) in {elapsed:?}");
}

fn configured_builder() -> BenchApiBuilder {
    BenchApi::builder()
        .api_secret("benchmark-only-key")
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
}

fn configured_credential_builder() -> CredentialBenchApiBuilder {
    CredentialBenchApi::builder().api_secret("benchmark-only-ready-credential")
}

fn core_client_construction(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let client = configured_builder().build().expect("benchmark client");
        black_box(client);
    }
    report(audit, "core client construction", iterations, started);
}

fn generated_client_facade_and_request_creation(iterations: usize, audit: &mut CategoryAudit) {
    let client = configured_builder().build().expect("benchmark client");
    let started = Instant::now();
    for index in 0..iterations {
        let facade = client.records("archive".to_owned());
        let request: Request<_> = facade.show(index.to_string());
        black_box(request);
    }
    report(
        audit,
        "generated client, facade, and request creation",
        iterations,
        started,
    );
}

fn public_head_preparation(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = configured_builder().build()?;
    let requests: Vec<_> = (0..iterations)
        .map(|index| client.records("archive".to_owned()).show(index.to_string()))
        .collect();
    let started = Instant::now();
    for request in requests {
        let observation = runtime.block_on(benchmark_public_head(request))?;
        assert_eq!(
            observation.operation(),
            BenchmarkOperation::PublicHeadPreparation
        );
        assert!(observation.work_units() > 0);
        black_box(observation);
    }
    report(audit, "public head preparation", iterations, started);
    Ok(())
}

fn codec_encoding_and_decoding(iterations: usize, audit: &mut CategoryAudit) {
    let method = Method::POST;
    let started = Instant::now();
    for _ in 0..iterations {
        let encoded = BenchCodec::encode(
            "benchmark payload".to_owned(),
            EncodeContext {
                endpoint: "BenchApi::codec",
                method: &method,
                body_limit: Some(1_024),
            },
        )
        .expect("benchmark encode");
        let decoded = BenchCodec::decode(
            encoded,
            DecodeContext {
                endpoint: "BenchApi::codec",
                method: &method,
                status: StatusCode::OK,
                content_type: Some("application/vnd.concord.benchmark"),
            },
        )
        .expect("benchmark decode");
        black_box(decoded);
    }
    report(audit, "codec encoding and decoding", iterations, started);
}

fn body_limit_polling(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = ScriptedServer::start((0..iterations).map(|_| ResponseScript::Chunked {
        status: 200,
        headers: Vec::new(),
        chunks: vec![vec![1; 16], vec![2; 16], vec![3; 16]],
    }))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = BenchApi::builder()
        .api_secret("benchmark-only-key")
        .http(move |http| http.proxy(proxy))
        .runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(NoopRateLimiter::new()))
                .response_body_limit(NonZeroUsize::new(64).expect("positive body limit"))
        })
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.bounded().into_future())?);
    }
    report(audit, "body-limit polling", iterations, started);
    Ok(())
}

fn credential_ready_path_preparation(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = configured_credential_builder().build()?;
    let requests: Vec<_> = (0..iterations).map(|_| client.credential_ready()).collect();
    let started = Instant::now();
    for request in requests {
        let observation = runtime.block_on(benchmark_ready_credentials(request))?;
        assert_eq!(
            observation.operation(),
            BenchmarkOperation::CredentialReadyPreparation
        );
        assert_eq!(observation.work_units(), 1);
        black_box(observation);
    }
    report(
        audit,
        "credential ready-path preparation and finalization",
        iterations,
        started,
    );
    Ok(())
}

fn limiter_ready_path_acquisition(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(b"limited".to_vec())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.limited().into_future())?);
    }
    report(audit, "limiter ready-path acquisition", iterations, started);
    Ok(())
}

fn pagination_loop_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    use bench_api::prelude::*;

    let scripts = (0..iterations).flat_map(|_| {
        [
            ResponseScript::ok(b"a".to_vec()),
            ResponseScript::ok(Vec::<u8>::new()),
        ]
    });
    let server = ScriptedServer::start(scripts)?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        let pages: PaginatedRequest<_> = client
            .pages(0, std::num::NonZeroU32::MIN)
            .paginate(PageLimit::Take(2));
        black_box(runtime.block_on(pages.into_future())?);
    }
    report(audit, "pagination loop overhead", iterations, started);
    Ok(())
}

fn stream_wrapping(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let stream =
            StreamBody::from_bytes(Bytes::from_static(b"stream benchmark")).exact_length(16);
        black_box(stream);
    }
    report(audit, "stream wrapping", iterations, started);
}

fn generic_specification_selection(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let endpoint = PreparedEndpoint::builder("BenchApi::generic", Method::POST)
            .body_spec::<BenchCodec>("benchmark generic body".to_owned())
            .response::<BenchCodec>()
            .build()
            .expect("generic benchmark endpoint");
        black_box(endpoint);
    }
    report(
        audit,
        "generic specification selection",
        iterations,
        started,
    );
}

fn record_framing(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let mut decoder = BenchRecords;
        let mut buffer = BytesMut::from(&b"first\nsecond\nthird"[..]);
        let mut count = 0;
        while decoder
            .decode(&mut buffer)
            .expect("benchmark record framing")
            .is_some()
        {
            count += 1;
        }
        if decoder
            .decode_eof(&mut buffer)
            .expect("benchmark record EOF")
            .is_some()
        {
            count += 1;
        }
        assert_eq!(count, 3);
        black_box(decoder);
    }
    report(audit, "record framing", iterations, started);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations();
    let runtime = tokio::runtime::Runtime::new()?;
    let mut audit = CategoryAudit::default();

    core_client_construction(iterations, &mut audit);
    generated_client_facade_and_request_creation(iterations, &mut audit);
    public_head_preparation(iterations, &runtime, &mut audit)?;
    codec_encoding_and_decoding(iterations, &mut audit);
    body_limit_polling(iterations, &runtime, &mut audit)?;
    credential_ready_path_preparation(iterations, &runtime, &mut audit)?;
    limiter_ready_path_acquisition(iterations, &runtime, &mut audit)?;
    pagination_loop_overhead(iterations, &runtime, &mut audit)?;
    stream_wrapping(iterations, &mut audit);
    generic_specification_selection(iterations, &mut audit);
    record_framing(iterations, &mut audit);
    audit.finish();

    black_box(Duration::ZERO);
    Ok(())
}
