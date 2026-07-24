#![forbid(unsafe_code)]

use std::io;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use concord_fixture_features::{EmptyGeneratedApi, LimitedGeneratedApi};
use runtime::Bytes;
use runtime::advanced::{
    BoxFuture, ClientBuildErrorKind, ErrorCategory, Method, NoopRateLimiter, RateLimitError,
    RateLimitId, RateLimitKey, RateLimitKeyPart, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimitRule, RateLimitWindow, RateLimiter, RuntimeEndpoint, SafeProxy,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration as TokioDuration, timeout};

const DEADLOCK_BOUND: TokioDuration = TokioDuration::from_secs(10);

#[derive(Clone, Debug, PartialEq, Eq)]
struct CapturedRule {
    id: String,
    parts: Vec<(String, String)>,
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
            .rules()
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

    fn observe<'a>(&'a self, _response: RateLimitResponse<'a>) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            self.observations.fetch_add(1, Ordering::SeqCst);
        })
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

fn runtime_rule(id: &str, key: RateLimitKey, cost: u32, windows: &[(u32, u64)]) -> RateLimitRule {
    RateLimitRule::new(
        RateLimitId::new(id).expect("runtime rate ID"),
        key,
        NonZeroU32::new(cost).expect("positive runtime cost"),
        windows.iter().map(|(capacity, seconds)| {
            RateLimitWindow::new(
                NonZeroU32::new(*capacity).expect("positive runtime capacity"),
                Duration::from_secs(*seconds),
            )
            .expect("runtime window")
        }),
    )
    .expect("runtime rule")
}

fn empty_runtime_endpoint() -> runtime::advanced::RuntimeEndpoint<Bytes> {
    RuntimeEndpoint::builder("runtime.empty".into(), Method::GET)
        .path_segment("runtime-empty")
        .response_bytes()
        .build()
        .expect("empty runtime endpoint")
}

fn explicit_runtime_endpoint() -> runtime::advanced::RuntimeEndpoint<Bytes> {
    RuntimeEndpoint::builder("runtime.explicit".into(), Method::GET)
        .path_segment("runtime-explicit")
        .rate_limit(runtime_rule(
            "Runtime::global",
            RateLimitKey::global(),
            2,
            &[(5, 1), (20, 10)],
        ))
        .rate_limit(runtime_rule(
            "Runtime::parts",
            RateLimitKey::from_parts([
                RateLimitKeyPart::new("tenant", "fixture-tenant").expect("tenant key part"),
                RateLimitKeyPart::new("region", "fixture-region").expect("region key part"),
            ])
            .expect("runtime explicit key"),
            3,
            &[(7, 2)],
        ))
        .response_bytes()
        .build()
        .expect("explicit runtime endpoint")
}

#[tokio::test]
async fn generated_clients_fail_closed_without_governor_unless_limiter_is_deliberate() {
    let server = LoopbackServer::start(6).await;

    let empty = EmptyGeneratedApi::builder()
        .configure_http({
            let proxy = server.proxy();
            move |http| http.proxy(proxy)
        })
        .build()
        .expect("empty generated client without governor");
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
    assert!(
        generated_plan[0][0]
            .id
            .ends_with("limited_generated_api.rate_limit")
    );
    assert!(generated_plan[0][0].parts.is_empty());
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
    empty_runtime_client
        .request(empty_runtime_endpoint())
        .await
        .expect("empty runtime fallback request");
    assert_eq!(server.request_count(), 4);

    let nonempty_fallback = empty_runtime_client
        .request(explicit_runtime_endpoint())
        .await;
    let error = nonempty_fallback.expect_err("runtime fallback must fail before send");
    assert_eq!(error.category(), ErrorCategory::RateLimit);
    assert_eq!(
        error.rate_limit_kind(),
        Some(runtime::advanced::RateLimitErrorKind::InvalidConfiguration)
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
        .request(explicit_runtime_endpoint())
        .await
        .expect("custom runtime request");
    assert_eq!(server.request_count(), 5);
    assert_eq!(
        runtime_limiter.plans.lock().expect("runtime capture")[0],
        [
            CapturedRule {
                id: "Runtime::global".to_owned(),
                parts: vec![],
                cost: 2,
                windows: vec![(5, Duration::from_secs(1)), (20, Duration::from_secs(10))]
            },
            CapturedRule {
                id: "Runtime::parts".to_owned(),
                parts: vec![
                    ("tenant".to_owned(), "fixture-tenant".to_owned()),
                    ("region".to_owned(), "fixture-region".to_owned())
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
        .request(explicit_runtime_endpoint())
        .await
        .expect("no-op runtime request");
    assert_eq!(server.request_count(), 6);
    server.finish().await;
    eprintln!("CONCORD_NO_GOVERNOR_RATE_LIMITS_OK generated=4 runtime=4 fallback_sends=0");
}
