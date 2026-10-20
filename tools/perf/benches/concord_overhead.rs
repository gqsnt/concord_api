#![forbid(unsafe_code)]

use std::collections::BTreeSet;
use std::future::IntoFuture;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use concord::api;
use concord::body::RequestStream;
use concord::codec::{
    BytesMut, CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext,
};
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::{Method, SafeProxy, StatusCode};
use concord::rate_limit::NoopRateLimiter;
use concord::records::{
    RecordDecoder, RecordError, decode_complete_records, validate_record_decoder,
};
use concord::response::{Buffered, ResponseBodyError, ResponseHead, ResponseRepresentation};
use concord::retry::{RetryDecision, RetryFailure, RetryPolicy};
use concord::{Binary, Bytes, SecretString};
use concord_perf::overhead_cli::{
    BUFFERED_RECORDS_CATEGORY, BenchmarkCommand, CategorySelection, MANAGED_RETRY_CATEGORY,
    help_text, list_text, parse_arguments,
};
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::server::{ResponseScript, ScriptedServer};
use concord_test_support::tls::TlsServer;

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
        String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))
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

    fn start(_head: &ResponseHead<'_>) -> Result<Self, RecordError> {
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
        NonZeroUsize::new(16).expect("positive benchmark staging bound")
    }
}

pub struct BenchRecordRepresentation;

impl ResponseRepresentation<Vec<Bytes>> for BenchRecordRepresentation {
    type Driver = Buffered;

    const ACCEPT: Option<&'static str> = BenchRecords::ACCEPT;

    fn validate(head: &ResponseHead<'_>) -> Result<(), ResponseBodyError> {
        validate_record_decoder::<BenchRecords>(head)
    }

    fn from_body(body: Bytes, head: &ResponseHead<'_>) -> Result<Vec<Bytes>, ResponseBodyError> {
        decode_complete_records::<BenchRecords>(body, head)
    }
}

api! {
    client BenchApi {
        timeout 30s;
        base "http://benchmark.example.test/";
        credentials {
            secret api_secret;
            credential application_key = bearer(secret.api_secret);
        }

        scope records(collection: String) {
            path "records/{collection}";

            GET show(id: String) -> Bytes via Binary {
                path "{id}";
                auth off;
            }
        }

        GET bounded() -> Bytes via Binary {
            auth off;
        }

        GET protected() -> Bytes via Binary {
            auth { bearer application_key; }
        }

        GET limited() -> Bytes via Binary {
            auth off;
            rate_limit {
                id "concord.concord_overhead.1";
                key [origin];
                cost 1;
                allow 1000000 per 1s;
            }
        }

        GET pages(query offset: u64, query limit: std::num::NonZeroU32) -> Vec<u8> via crate::PageCodec {
            auth off;
            paginate via concord::pagination::OffsetPagination {
                init concord::pagination::OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }

        POST codec(body body: String via crate::BenchCodec) -> String via crate::BenchCodec {
            auth off;
        }

        GET streamed() -> ::concord::response::ByteStream via Binary {
            auth off;
        }

        GET buffered_records() -> Vec<Bytes> via crate::BenchRecordRepresentation {
            auth off;
        }
    }
}

api! {
    client CredentialBenchApi {
        timeout 30s;
        base "https://localhost/";
        credentials {
            secret api_secret;
            credential application_key = bearer(secret.api_secret);
        }

        GET credential_ready() -> Bytes via Binary {
            auth { bearer application_key; }
        }
    }
}

const EXPECTED_CATEGORIES: [&str; 17] = [
    "core client construction",
    "generated endpoint construction",
    "request binding",
    "safe public planning",
    "codec encoding and decoding",
    "body-limit polling",
    "direct execution overhead",
    MANAGED_RETRY_CATEGORY,
    "explicit generated endpoint execution overhead",
    "runtime endpoint execution overhead",
    "authenticated managed execution",
    "rate-limited managed execution",
    "pagination loop overhead",
    "stream wrapping",
    "generic specification selection",
    "record framing",
    BUFFERED_RECORDS_CATEGORY,
];

struct CategoryAudit {
    expected: &'static [&'static str],
    executed: BTreeSet<&'static str>,
}

impl CategoryAudit {
    fn new(selection: CategorySelection) -> Self {
        const MANAGED_RETRY_ONLY: [&str; 1] = [MANAGED_RETRY_CATEGORY];
        const BUFFERED_RECORDS_ONLY: [&str; 1] = [BUFFERED_RECORDS_CATEGORY];
        let expected: &'static [&'static str] = match selection {
            CategorySelection::All => &EXPECTED_CATEGORIES,
            CategorySelection::ManagedRetry => &MANAGED_RETRY_ONLY,
            CategorySelection::BufferedRecords => &BUFFERED_RECORDS_ONLY,
        };
        Self {
            expected,
            executed: BTreeSet::new(),
        }
    }

    fn record(&mut self, name: &'static str) {
        assert!(
            self.expected.contains(&name),
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
            self.expected.len(),
            "not every benchmark category executed"
        );
        for expected in self.expected {
            assert!(
                self.executed.contains(expected),
                "benchmark category did not execute: {expected}"
            );
        }
        println!(
            "benchmark operation audit: {0}/{0} named categories executed",
            self.expected.len()
        );
    }
}

fn configured_iterations() -> usize {
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

fn configured_builder() -> bench_api::BenchApiBuilder {
    BenchApi::builder()
        .api_secret(SecretString::try_new("benchmark-only-key").expect("bounded benchmark key"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
}

fn configured_credential_builder() -> credential_bench_api::CredentialBenchApiBuilder {
    CredentialBenchApi::builder().api_secret(
        SecretString::try_new("benchmark-only-ready-credential")
            .expect("bounded benchmark credential"),
    )
}

fn core_client_construction(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let client = configured_builder().build().expect("benchmark client");
        black_box(client);
    }
    report(audit, "core client construction", iterations, started);
}

fn generated_endpoint_construction(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for index in 0..iterations {
        let endpoint = BenchApi::unbound()
            .records("archive".to_owned())
            .show(index.to_string());
        black_box(endpoint);
    }
    report(
        audit,
        "generated endpoint construction",
        iterations,
        started,
    );
}

fn request_binding(iterations: usize, audit: &mut CategoryAudit) {
    let client = configured_builder().build().expect("benchmark client");
    let endpoints = (0..iterations)
        .map(|index| {
            BenchApi::unbound()
                .records("archive".to_owned())
                .show(index.to_string())
        })
        .collect::<Vec<_>>();
    let started = Instant::now();
    for endpoint in endpoints {
        let request: bench_api::records::ShowRequest = client.request(endpoint);
        let _ = black_box(request);
    }
    report(audit, "request binding", iterations, started);
}

fn safe_public_planning(
    iterations: usize,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = configured_builder().build()?;
    let endpoints = (0..iterations)
        .map(|index| {
            BenchApi::unbound()
                .records("archive".to_owned())
                .show(index.to_string())
        })
        .collect::<Vec<_>>();
    let started = Instant::now();
    for endpoint in &endpoints {
        black_box(client.plan(endpoint)?);
    }
    report(audit, "safe public planning", iterations, started);
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
        .api_secret(SecretString::try_new("benchmark-only-key").expect("bounded benchmark key"))
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(|runtime| {
            runtime
                .rate_limiter(Arc::new(NoopRateLimiter::new()))
                .response_body_limit(64)
        })
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.bounded().into_future())?);
    }
    report(audit, "body-limit polling", iterations, started);
    Ok(())
}

fn direct_execution_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(b"direct".to_vec())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.bounded().into_future())?);
    }
    report(audit, "direct execution overhead", iterations, started);
    Ok(())
}

fn explicit_generated_endpoint_execution_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(b"explicit".to_vec())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let endpoints = (0..iterations)
        .map(|_| BenchApi::unbound().bounded())
        .collect::<Vec<_>>();
    let started = Instant::now();
    for endpoint in endpoints {
        black_box(runtime.block_on(client.request(endpoint).into_future())?);
    }
    report(
        audit,
        "explicit generated endpoint execution overhead",
        iterations,
        started,
    );
    Ok(())
}

fn runtime_endpoint_execution_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(b"runtime".to_vec())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let endpoints = (0..iterations)
        .map(|index| {
            RuntimeEndpoint::builder(
                EndpointId::new(format!("bench_api.runtime_{index}"))?,
                Method::GET,
            )
            .path("runtime")
            .response::<concord::Binary, concord::Bytes>()
            .build()
        })
        .collect::<Result<Vec<_>, _>>()?;
    let started = Instant::now();
    for endpoint in endpoints {
        // v3 binds a runtime endpoint at the owner projection (`owner.bind`).
        let request = client.bind(endpoint)?;
        black_box(runtime.block_on(request.into_future())?);
    }
    report(
        audit,
        "runtime endpoint execution overhead",
        iterations,
        started,
    );
    Ok(())
}

fn managed_retry_policy(classifications: Arc<AtomicUsize>) -> RetryPolicy {
    RetryPolicy::builder()
        .max_retries_per_operation(1)
        .without_budget()
        .classify_fn(move |attempt| {
            classifications.fetch_add(1, Ordering::SeqCst);
            match attempt.failure() {
                RetryFailure::RejectedStatus { status, .. }
                    if status == StatusCode::SERVICE_UNAVAILABLE =>
                {
                    RetryDecision::Retry
                }
                _ => RetryDecision::Stop,
            }
        })
        .build()
        .expect("valid managed-retry benchmark policy")
}

fn managed_retry_client(
    server: &ScriptedServer,
    classifications: Arc<AtomicUsize>,
) -> Result<BenchApi, Box<dyn std::error::Error>> {
    let proxy = SafeProxy::parse(server.url(""))?;
    Ok(configured_builder()
        .configure_http(move |http| http.proxy(proxy).protocol_recovery(false))
        .retry(managed_retry_policy(classifications))
        .build()?)
}

fn prove_managed_retry_scenario(
    runtime: &tokio::runtime::Runtime,
) -> Result<(), Box<dyn std::error::Error>> {
    let server = ScriptedServer::start([
        ResponseScript::status(503, b"proof-rejected".to_vec()),
        ResponseScript::ok(b"proof-success".to_vec()),
    ])?;
    let classifications = Arc::new(AtomicUsize::new(0));
    let client = managed_retry_client(&server, Arc::clone(&classifications))?;

    let response = runtime.block_on(client.bounded().response().into_future())?;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.value(), &Bytes::from_static(b"proof-success"));
    assert_eq!(response.metadata().attempts_started(), 2);
    assert_eq!(response.metadata().policy_retries(), 1);
    assert!(!response.metadata().recovered_authentication());
    assert_eq!(classifications.load(Ordering::SeqCst), 1);
    assert_eq!(server.request_count(), 2);
    Ok(())
}

fn managed_retry_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    prove_managed_retry_scenario(runtime)?;

    let scripts = (0..iterations).flat_map(|_| {
        [
            ResponseScript::status(503, b"retry".to_vec()),
            ResponseScript::ok(b"ok".to_vec()),
        ]
    });
    let server = ScriptedServer::start(scripts)?;
    let classifications = Arc::new(AtomicUsize::new(0));
    let client = managed_retry_client(&server, Arc::clone(&classifications))?;
    let expected_physical_sends = iterations
        .checked_mul(2)
        .expect("bounded benchmark send count");

    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.bounded().response().into_future())?);
    }
    let elapsed = started.elapsed();

    assert_eq!(classifications.load(Ordering::SeqCst), iterations);
    assert_eq!(server.request_count(), expected_physical_sends);
    audit.record(MANAGED_RETRY_CATEGORY);
    let divisor = u32::try_from(iterations).expect("bounded benchmark iteration count");
    println!(
        "{MANAGED_RETRY_CATEGORY}: {iterations} logical operation(s), one managed retry per \
         operation, {expected_physical_sends} expected physical application send(s), {elapsed:?} \
         total, {:?} per operation",
        elapsed / divisor
    );
    Ok(())
}

fn authenticated_managed_execution(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let tls = TlsServer::start().map_err(|error| std::io::Error::other(error.to_string()))?;
    let proxy = ConnectProxy::start(tls.address())?;
    let safe_proxy = SafeProxy::parse(proxy.url())?;
    let certificate = tls.certificate_pem();
    let client = configured_credential_builder()
        .configure_http(move |http| {
            http.http1_only()
                .proxy(safe_proxy)
                .add_root_certificate_pem(certificate)
        })
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.credential_ready().into_future())?);
    }
    report(
        audit,
        "authenticated managed execution",
        iterations,
        started,
    );

    tls.wait_for_requests(iterations, Duration::from_secs(5))
        .map_err(std::io::Error::other)?;
    let requests = tls.requests();
    assert_eq!(requests.len(), iterations);
    for request in requests {
        assert_eq!(
            request.header("authorization"),
            Some("Bearer benchmark-only-ready-credential")
        );
    }
    Ok(())
}

fn rate_limited_managed_execution(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let server =
        ScriptedServer::start((0..iterations).map(|_| ResponseScript::ok(b"limited".to_vec())))?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        black_box(runtime.block_on(client.limited().into_future())?);
    }
    report(audit, "rate-limited managed execution", iterations, started);
    Ok(())
}

fn pagination_loop_overhead(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    let scripts = (0..iterations).flat_map(|_| {
        [
            ResponseScript::ok(b"a".to_vec()),
            ResponseScript::ok(Vec::<u8>::new()),
        ]
    });
    let server = ScriptedServer::start(scripts)?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;
    let started = Instant::now();
    for _ in 0..iterations {
        // A bounded prefix is selected explicitly with the V3
        // terminal: at most two pages, the same two sends per iteration.
        let prefix = client.pages(0, std::num::NonZeroU32::MIN).collect_prefix(2);
        black_box(runtime.block_on(prefix.into_future())?);
    }
    report(audit, "pagination loop overhead", iterations, started);
    Ok(())
}

fn stream_wrapping(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let stream =
            RequestStream::from_bytes(Bytes::from_static(b"stream benchmark")).exact_length(16);
        black_box(stream);
    }
    report(audit, "stream wrapping", iterations, started);
}

fn generic_specification_selection(iterations: usize, audit: &mut CategoryAudit) {
    let started = Instant::now();
    for _ in 0..iterations {
        let endpoint = RuntimeEndpoint::builder(
            EndpointId::new("bench_api.runtime").expect("valid benchmark endpoint ID"),
            Method::POST,
        )
        .body_repr::<BenchCodec, _>("benchmark generic body".to_owned())
        .response::<BenchCodec, String>()
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

fn buffered_record_collection(
    iterations: usize,
    runtime: &tokio::runtime::Runtime,
    audit: &mut CategoryAudit,
) -> Result<(), Box<dyn std::error::Error>> {
    const RECORDS_PER_RESPONSE: usize = 256;
    let body = b"record\n".repeat(RECORDS_PER_RESPONSE);
    let scripts = (0..iterations).map(|_| ResponseScript::ok(body.clone()));
    let server = ScriptedServer::start(scripts)?;
    let proxy = SafeProxy::parse(server.url(""))?;
    let client = configured_builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()?;

    let started = Instant::now();
    for _ in 0..iterations {
        let records = runtime.block_on(client.buffered_records().into_future())?;
        assert_eq!(
            records.len(),
            RECORDS_PER_RESPONSE,
            "buffered record benchmark decoded an incomplete response"
        );
        black_box(records);
    }
    report(audit, BUFFERED_RECORDS_CATEGORY, iterations, started);
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let command =
        parse_arguments(&arguments, configured_iterations()).map_err(std::io::Error::other)?;
    let options = match command {
        BenchmarkCommand::Help => {
            print!("{}", help_text());
            return Ok(());
        }
        BenchmarkCommand::List => {
            print!("{}", list_text());
            return Ok(());
        }
        BenchmarkCommand::Run(options) => options,
    };
    let iterations = options.iterations;
    let runtime = tokio::runtime::Runtime::new()?;
    let mut audit = CategoryAudit::new(options.selection);

    match options.selection {
        CategorySelection::ManagedRetry => {
            managed_retry_overhead(iterations, &runtime, &mut audit)?;
            audit.finish();
            return Ok(());
        }
        CategorySelection::BufferedRecords => {
            buffered_record_collection(iterations, &runtime, &mut audit)?;
            audit.finish();
            return Ok(());
        }
        CategorySelection::All => {}
    }

    core_client_construction(iterations, &mut audit);
    generated_endpoint_construction(iterations, &mut audit);
    request_binding(iterations, &mut audit);
    safe_public_planning(iterations, &mut audit)?;
    codec_encoding_and_decoding(iterations, &mut audit);
    body_limit_polling(iterations, &runtime, &mut audit)?;
    direct_execution_overhead(iterations, &runtime, &mut audit)?;
    managed_retry_overhead(iterations, &runtime, &mut audit)?;
    explicit_generated_endpoint_execution_overhead(iterations, &runtime, &mut audit)?;
    runtime_endpoint_execution_overhead(iterations, &runtime, &mut audit)?;
    authenticated_managed_execution(iterations, &runtime, &mut audit)?;
    rate_limited_managed_execution(iterations, &runtime, &mut audit)?;
    pagination_loop_overhead(iterations, &runtime, &mut audit)?;
    stream_wrapping(iterations, &mut audit);
    generic_specification_selection(iterations, &mut audit);
    record_framing(iterations, &mut audit);
    buffered_record_collection(iterations, &runtime, &mut audit)?;
    audit.finish();

    black_box(Duration::ZERO);
    Ok(())
}
