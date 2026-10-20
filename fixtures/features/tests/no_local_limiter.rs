#![forbid(unsafe_code)]

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_fixture_features::{EmptyGeneratedApi, LimitedGeneratedApi};
use runtime::Bytes;
use runtime::endpoint::{EndpointId, RuntimeEndpoint};
use runtime::error::{ClientBuildErrorKind, ErrorCategory};
use runtime::http::{Method, SafeProxy};
use runtime::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, NoopRateLimiter, RateLimitDeclaration,
    RateLimitError, RateLimitPermit, RateLimitRequest, RateLimitResponse, RateLimiter,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration as TokioDuration, timeout};

const DEADLOCK_BOUND: TokioDuration = TokioDuration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRule {
    id: String,
    parts: Vec<(String, Vec<u8>)>,
    cost: u32,
    windows: Vec<(u32, Duration)>,
}

#[derive(Default)]
struct CapturingLimiter {
    plans: Mutex<Vec<Vec<CapturedRule>>>,
    observations: AtomicUsize,
}

impl CapturingLimiter {
    fn capture(request: &RateLimitRequest<'_>) -> Vec<CapturedRule> {
        request
            .plan
            .policies()
            .iter()
            .map(|rule| CapturedRule {
                id: rule.id().as_str().to_owned(),
                parts: rule
                    .key()
                    .parts()
                    .iter()
                    .map(|part| (part.name().to_owned(), part.value().to_owned()))
                    .collect(),
                cost: rule.cost().get(),
                windows: rule
                    .windows()
                    .iter()
                    .map(|window| (window.capacity().get(), window.duration()))
                    .collect(),
            })
            .collect()
    }
}

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, "concord.fixture.capturing", 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        let plan = Self::capture(&request);
        Box::pin(async move {
            self.plans.lock().expect("capture lock").push(plan);
            Ok(RateLimitPermit::granted())
        })
    }

    fn observe(&self, _response: RateLimitResponse<'_>) {
        self.observations.fetch_add(1, Ordering::SeqCst);
    }
}

struct LoopbackServer {
    address: std::net::SocketAddr,
    requests: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl LoopbackServer {
    async fn start(expected: usize) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("ephemeral loopback server");
        let address = listener.local_addr().expect("loopback address");
        let requests = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&requests);
        let task = tokio::spawn(async move {
            for _ in 0..expected {
                let (mut stream, _) = timeout(DEADLOCK_BOUND, listener.accept())
                    .await
                    .expect("loopback accept deadline")
                    .expect("loopback accept");
                read_request(&mut stream).await.expect("loopback request");
                count.fetch_add(1, Ordering::SeqCst);
                let response =
                    b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
                stream.write_all(response).await.expect("loopback response");
                stream.shutdown().await.expect("loopback shutdown");
            }
        });
        Self {
            address,
            requests,
            task,
        }
    }

    fn proxy(&self) -> SafeProxy {
        SafeProxy::parse(format!("http://{}", self.address)).expect("loopback proxy")
    }

    fn request_count(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }

    async fn finish(self) {
        timeout(DEADLOCK_BOUND, self.task)
            .await
            .expect("loopback server deadline")
            .expect("loopback server task");
    }
}

async fn read_request(stream: &mut TcpStream) -> io::Result<()> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "request ended",
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > 16 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "request too large",
            ));
        }
    }
    Ok(())
}

/// One runtime rate declaration: the built-in `origin` dimension when
/// `origin` is set, then the labelled custom parts, with its cost and windows.
fn runtime_rule(
    id: &str,
    origin: bool,
    parts: &[(&str, &str)],
    cost: u32,
    windows: &[(u32, u64)],
) -> RateLimitDeclaration {
    let mut declaration = RateLimitDeclaration::new(id).cost(cost);
    for (capacity, seconds) in windows {
        declaration = declaration.window(*capacity, Duration::from_secs(*seconds));
    }
    if origin {
        declaration = declaration.origin();
    }
    for (label, value) in parts {
        declaration = declaration.part(label, *value);
    }
    declaration.validate().expect("runtime rule");
    declaration
}

fn canonical_text(value: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1 + 4 + 8 + value.len());
    bytes.push(4);
    bytes.extend_from_slice(b"utf8");
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn empty_runtime_endpoint() -> RuntimeEndpoint<runtime::Binary, Bytes> {
    RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.empty").expect("runtime endpoint ID"),
        Method::GET,
    )
    .path("runtime-empty")
    // v2.1 `.accepted_bytes()` selected the raw `Binary`/`Bytes` output; v3
    // selects the output pair explicitly (§16.1 `.response::<R, O>()`).
    .response::<runtime::Binary, Bytes>()
    .build()
    .expect("empty runtime endpoint")
}

fn explicit_runtime_endpoint() -> RuntimeEndpoint<runtime::Binary, Bytes> {
    RuntimeEndpoint::builder(
        EndpointId::try_from("runtime.explicit").expect("runtime endpoint ID"),
        Method::GET,
    )
    .path("runtime-explicit")
    // v2.1 allowed a keyless (global) runtime rule; the v3 declaration
    // rejects an empty key (`RateLimitDeclaration::validate` -> InvalidKey),
    // so the first rule is keyed by the built-in logical `origin` dimension.
    .rate_limit(runtime_rule(
        "Runtime::origin",
        true,
        &[],
        2,
        &[(5, 1), (20, 10)],
    ))
    .rate_limit(runtime_rule(
        "Runtime::parts",
        false,
        &[("tenant", "fixture-tenant"), ("region", "fixture-region")],
        3,
        &[(7, 2)],
    ))
    .response::<runtime::Binary, Bytes>()
    .build()
    .expect("explicit runtime endpoint")
}

#[tokio::test]
async fn generated_clients_fail_closed_without_local_limiter_unless_limiter_is_deliberate() {
    let server = LoopbackServer::start(6).await;

    let empty = EmptyGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .build()
        .expect("empty generated client without the local limiter");
    assert_eq!(
        empty.read().await.expect("empty generated request"),
        Bytes::from_static(b"ok")
    );
    assert_eq!(server.request_count(), 1);

    let fallback = match LimitedGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .build()
    {
        Ok(_) => panic!("nonempty generated fallback unexpectedly built"),
        Err(error) => error,
    };
    assert_eq!(fallback.kind(), ClientBuildErrorKind::LimiterUnavailable);
    assert_eq!(server.request_count(), 1);

    let limiter = Arc::new(CapturingLimiter::default());
    let custom = LimitedGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("custom generated limiter");
    assert_eq!(
        custom.read().await.expect("custom generated request"),
        Bytes::from_static(b"ok")
    );
    assert_eq!(server.request_count(), 2);
    assert_eq!(limiter.observations.load(Ordering::SeqCst), 1);
    let generated_plan = limiter.plans.lock().expect("generated plan capture");
    assert_eq!(generated_plan.len(), 1);
    assert_eq!(generated_plan[0].len(), 1);
    assert_eq!(generated_plan[0][0].id, "fixture.generated.limit");
    assert_eq!(
        generated_plan[0][0].parts,
        [(
            "origin".to_owned(),
            canonical_text("http://no-local-limiter-limited.example.test:80"),
        )]
    );
    assert_eq!(generated_plan[0][0].cost, 2);
    assert_eq!(
        generated_plan[0][0].windows,
        [(3, Duration::from_secs(60)), (30, Duration::from_secs(600))]
    );
    drop(generated_plan);

    let noop = LimitedGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("explicit generated no-op");
    noop.read().await.expect("no-op generated request");
    assert_eq!(server.request_count(), 3);

    let empty_runtime_client = EmptyGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .build()
        .expect("empty runtime fallback client");
    // v3 binds runtime endpoints at the owner projection (`owner.bind`).
    empty_runtime_client
        .bind(empty_runtime_endpoint())
        .expect("empty runtime endpoint binds")
        .await
        .expect("empty runtime fallback request");
    assert_eq!(server.request_count(), 4);

    // v2.1 bound a rate-limited runtime endpoint on a client without a
    // limiter selection and failed its execution before send with
    // RateLimit/InvalidConfiguration. v3 (RATE-01, V3-R11: a missing limiter
    // cannot bypass build/bind) rejects the binding itself, so the fallback
    // still never reaches a send.
    let error = match empty_runtime_client.bind(explicit_runtime_endpoint()) {
        Ok(_) => panic!("a rate-limited runtime endpoint must not bind without a limiter"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(
        error.configuration_kind(),
        Some(runtime::error::ConfigurationErrorKind::InvalidRateLimit)
    );
    assert_eq!(
        error.dispatch_knowledge(),
        runtime::DispatchKnowledge::NotDispatched
    );
    assert_eq!(server.request_count(), 4);

    let runtime_limiter = Arc::new(CapturingLimiter::default());
    let custom_runtime_client = EmptyGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .configure_runtime({
            let limiter = Arc::clone(&runtime_limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("custom runtime limiter client");
    custom_runtime_client
        .bind(explicit_runtime_endpoint())
        .expect("rate-limited runtime endpoint binds")
        .await
        .expect("custom runtime request");
    assert_eq!(server.request_count(), 5);
    assert_eq!(
        runtime_limiter.plans.lock().expect("runtime capture")[0],
        [
            CapturedRule {
                id: "Runtime::origin".to_owned(),
                parts: vec![(
                    "origin".to_owned(),
                    canonical_text("http://no-local-limiter-empty.example.test:80"),
                )],
                cost: 2,
                windows: vec![(5, Duration::from_secs(1)), (20, Duration::from_secs(10))]
            },
            CapturedRule {
                id: "Runtime::parts".to_owned(),
                parts: vec![
                    ("tenant".to_owned(), canonical_text("fixture-tenant")),
                    ("region".to_owned(), canonical_text("fixture-region"))
                ],
                cost: 3,
                windows: vec![(7, Duration::from_secs(2))]
            },
        ]
    );

    let noop_runtime_client = EmptyGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("no-op runtime client");
    noop_runtime_client
        .bind(explicit_runtime_endpoint())
        .expect("rate-limited runtime endpoint binds")
        .await
        .expect("no-op runtime request");
    assert_eq!(server.request_count(), 6);
    server.finish().await;
    eprintln!("CONCORD_NO_LOCAL_LIMITER_RATE_LIMITS_OK generated=4 runtime=4 fallback_sends=0");
}
