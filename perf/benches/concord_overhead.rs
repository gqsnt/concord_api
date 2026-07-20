#![forbid(unsafe_code)]

use std::future::IntoFuture;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use concord_core::advanced::{
    CodecError, DecodeBody, DecodeContext, EncodeBody, EncodeContext, Method, NoopRateLimiter,
    PreparedEndpoint, SafeProxy, StatusCode,
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
        GET Pages(offset: u64, limit: u32) -> crate::PageCodec {
            query { "offset" = ep.offset; "limit" = ep.limit; }
            auth off;
            rate_limit off;
            paginate OffsetPagination { offset = ep.offset, page_size = ep.limit }
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

fn report(name: &str, iterations: usize, started: Instant) {
    let elapsed = started.elapsed();
    println!("{name}: {iterations} iteration(s) in {elapsed:?}");
}

fn configured_builder() -> BenchApiBuilder {
    BenchApi::builder()
        .api_secret("benchmark-only-key")
        .runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
}

fn core_client_construction(iterations: usize) {
    let started = Instant::now();
    for _ in 0..iterations {
        let client = configured_builder().build().expect("benchmark client");
        black_box(client);
    }
    report("core client construction", iterations, started);
}

fn generated_client_facade_and_request_creation(iterations: usize) {
    let client = configured_builder().build().expect("benchmark client");
    let started = Instant::now();
    for index in 0..iterations {
        let facade = client.records("archive".to_owned());
        let request: Request<_> = facade.show(index.to_string());
        black_box(request);
    }
    report(
        "generated client, facade, and request creation",
        iterations,
        started,
    );
}

fn public_head_preparation(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(Vec::<u8>::new())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for index in 0..iterations {
        let endpoint = PreparedEndpoint::builder("BenchApi::prepared", Method::GET)
            .path_segment("prepared")
            .query("iteration", index)
            .response_bytes()
            .build()?;
        black_box(runtime.block_on(client.request(endpoint).into_future())?);
    }
    report("public head preparation", iterations, started);
    Ok(())
}

fn codec_encoding_and_decoding(iterations: usize) {
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
    report("codec encoding and decoding", iterations, started);
}

fn body_limit_polling(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
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
    report("body-limit polling", iterations, started);
    Ok(())
}

fn credential_ready_path_preparation(iterations: usize) {
    let client = configured_builder().build().expect("benchmark client");
    let started = Instant::now();
    for _ in 0..iterations {
        let handle = client.credentials().application_key();
        black_box(handle.credential());
    }
    report("credential ready-path preparation", iterations, started);
}

fn limiter_ready_path_acquisition(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
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
    report("limiter ready-path acquisition", iterations, started);
    Ok(())
}

fn pagination_loop_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
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
        let pages: PaginatedRequest<_> = client.pages(0, 1).paginate(PageLimit::Take(2));
        black_box(runtime.block_on(pages.into_future())?);
    }
    report("pagination loop overhead", iterations, started);
    Ok(())
}

fn stream_wrapping(iterations: usize) {
    let started = Instant::now();
    for _ in 0..iterations {
        let stream =
            StreamBody::from_bytes(Bytes::from_static(b"stream benchmark")).exact_length(16);
        black_box(stream);
    }
    report("stream wrapping", iterations, started);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations();
    let runtime = tokio::runtime::Runtime::new()?;

    core_client_construction(iterations);
    generated_client_facade_and_request_creation(iterations);
    public_head_preparation(iterations, &runtime)?;
    codec_encoding_and_decoding(iterations);
    body_limit_polling(iterations, &runtime)?;
    credential_ready_path_preparation(iterations);
    limiter_ready_path_acquisition(iterations, &runtime)?;
    pagination_loop_overhead(iterations, &runtime)?;
    stream_wrapping(iterations);

    black_box(Duration::ZERO);
    Ok(())
}
