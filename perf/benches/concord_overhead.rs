#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::future::IntoFuture;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord_core::__development::{
    BenchmarkOperation, benchmark_public_head, benchmark_ready_credentials,
};
use concord_core::advanced::{
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, Method, NoopRateLimiter,
    SafeProxy, StatusCode,
};
use concord_core::prelude::{Bytes, PageLimit, StreamBody};
use concord_core::{PaginatedRequest, Request};
use concord_macros::api;
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

api! {
    client BenchApi {
        base "http://benchmark.example.test/";
        credentials {
            secret api_secret: String;
            credential application_key = bearer(secret.api_secret);
        }
        rate_limits {
            ready { by [endpoint]; 1000000 / 1s; }
        }
        scope Records(collection: String) {
            path ["records", scope.collection];
            GET Show(id: String) -> Bytes {
                path [ep.id];
                auth off;
                rate_limit off;
            }
        }
        GET Bounded() -> Bytes {
            auth off;
            rate_limit off;
        }
        GET Protected() -> Bytes {
            auth bearer application_key;
            rate_limit off;
        }
        GET Limited() -> Bytes {
            auth off;
            rate_limit ready;
        }
        GET Pages(offset: u64, limit: std::num::NonZeroU32) -> crate::PageCodec {
            query { "offset" = ep.offset; "limit" = ep.limit; }
            auth off;
            rate_limit off;
            paginate concord_core::advanced::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.limit,
            }
        }
        POST Codec(body: crate::BenchCodec) -> crate::BenchCodec {
            auth off;
            rate_limit off;
        }
        GET Streamed() -> Stream<OctetStream> {
            auth off;
            rate_limit off;
        }
    }
}

api! {
    client CredentialBenchApi {
        base "https://credential-benchmark.example.test/";
        credentials {
            secret api_secret: String;
            credential application_key = bearer(secret.api_secret);
        }
        GET CredentialReady() -> Bytes {
            auth bearer application_key;
            rate_limit off;
        }
    }
}

const EXPECTED_CATEGORIES: [&str; 9] = [
    "core client construction",
    "generated client, facade, and request creation",
    "public head preparation",
    "codec encoding and decoding",
    "body-limit polling",
    "credential ready-path preparation and finalization",
    "limiter ready-path acquisition",
    "pagination loop overhead",
    "stream wrapping",
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
        println!("benchmark operation audit: 9/9 named categories executed");
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
    audit.finish();

    black_box(Duration::ZERO);
    Ok(())
}
