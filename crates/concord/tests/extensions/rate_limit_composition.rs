//! Limiter selection composes through the public runtime builder facade:
//! a nonempty generated plan never falls back to a private limiter, and the
//! last explicit selection is the one the built client uses (§7.3, §12.5).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::error::ClientBuildErrorKind;
use concord::http::SafeProxy;
use concord::rate_limit::{
    BoxFuture, Enforcement, LimiterCapability, RateLimitError, RateLimitPermit, RateLimitRequest,
    RateLimitResponse, RateLimiter,
};
use concord::{Binary, Bytes, api};
use concord_test_support::server::{ResponseScript, ScriptedServer};

api! {
    client DefaultLocalLimiterApi {
        timeout 30s;
        base "http://default-local-limiter.example.test/";

        rate_limit { id "concord.rate_limit_composition.1"; key [origin]; cost 1; allow 10 per 1s; }

        GET read() -> Bytes via Binary;
    }
}

api! {
    client ExplicitLimiterApi {
        timeout 30s;
        base "http://explicit-limiter.example.test/";

        rate_limit { id "concord.rate_limit_composition.2"; key [origin]; cost 1; allow 10 per 1s; }

        GET read() -> Bytes via Binary;
    }
}

struct CapturingLimiter {
    namespace: &'static str,
    acquisitions: Arc<AtomicUsize>,
    observations: Arc<AtomicUsize>,
}

impl RateLimiter for CapturingLimiter {
    fn capability(&self) -> LimiterCapability {
        LimiterCapability::new(Enforcement::Local, self.namespace, 1)
    }

    fn acquire<'a>(
        &'a self,
        request: RateLimitRequest<'a>,
    ) -> BoxFuture<'a, Result<RateLimitPermit, RateLimitError>> {
        assert_eq!(request.capability.namespace(), self.namespace);
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(RateLimitPermit::granted()) })
    }

    fn observe(&self, response: RateLimitResponse<'_>) {
        assert_eq!(response.cooldown_cap, std::time::Duration::from_secs(60));
        self.observations.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn generated_nonempty_plan_fails_closed_without_an_explicit_limiter() {
    let error = match DefaultLocalLimiterApi::builder().build() {
        Ok(_) => panic!("missing generated limiter unexpectedly built"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), ClientBuildErrorKind::LimiterUnavailable);
}

#[tokio::test]
async fn explicit_limiter_wins_over_replaced_runtime_configuration() {
    let replaced_acquisitions = Arc::new(AtomicUsize::new(0));
    let replaced_observations = Arc::new(AtomicUsize::new(0));
    let replaced_limiter: Arc<dyn RateLimiter> = Arc::new(CapturingLimiter {
        namespace: "concord.test.replaced",
        acquisitions: Arc::clone(&replaced_acquisitions),
        observations: Arc::clone(&replaced_observations),
    });
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let observations = Arc::new(AtomicUsize::new(0));
    let limiter: Arc<dyn RateLimiter> = Arc::new(CapturingLimiter {
        namespace: "concord.test.selected",
        acquisitions: Arc::clone(&acquisitions),
        observations: Arc::clone(&observations),
    });
    let server = ScriptedServer::start([ResponseScript::ok(b"explicit".to_vec())]).expect("server");
    let proxy = SafeProxy::parse(server.url("")).expect("proxy");
    let client = ExplicitLimiterApi::builder()
        .configure_http(move |http| http.proxy(proxy))
        .configure_runtime(move |runtime| runtime.rate_limiter(replaced_limiter))
        .configure_runtime(move |runtime| runtime.rate_limiter(limiter))
        .build()
        .expect("explicit limiter client");

    // The plan discloses the capability snapshotted from the final selection.
    let plan = client.read().plan().expect("plan");
    assert_eq!(
        plan.rate_limit()
            .limiter()
            .map(|capability| capability.namespace()),
        Some("concord.test.selected")
    );
    assert_eq!(replaced_acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(replaced_observations.load(Ordering::SeqCst), 0);
    assert_eq!(
        client.read().await.expect("explicit-limited request"),
        Bytes::from_static(b"explicit")
    );
    assert_eq!(acquisitions.load(Ordering::SeqCst), 1);
    assert_eq!(observations.load(Ordering::SeqCst), 1);
    assert_eq!(replaced_acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(replaced_observations.load(Ordering::SeqCst), 0);
    assert_eq!(server.request_count(), 1);
}
