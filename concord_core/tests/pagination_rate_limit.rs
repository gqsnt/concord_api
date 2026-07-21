#![cfg(feature = "rate-limit-governor")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord_core::advanced::{
    BoxFuture, CodecError, DecodeBody, DecodeContext, GovernorRateLimiter, HookError,
    PaginationController, PaginationError, RateLimiter, RuntimeHooks, SafeProxy,
};
use concord_core::prelude::Bytes;
use concord_macros::api;
use concord_test_support::server::{ResponseScript, ScriptedServer};

pub struct KeyPageCodec;

impl DecodeBody for KeyPageCodec {
    type Output = Vec<u32>;

    const ACCEPT: Option<&'static str> = None;

    fn validate_content_type(_content_type: Option<&str>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|_| CodecError::new("page utf8"))?;
        text.split(',')
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u32>()
                    .map_err(|_| CodecError::new("page item"))
            })
            .collect()
    }
}

pub struct MultiPageKeyController {
    pub key: String,
}

impl PaginationController<Vec<u32>> for MultiPageKeyController {
    fn next(self, _page: &Vec<u32>) -> Result<Option<Self>, PaginationError> {
        if self.key.ends_with("-page-1") {
            Ok(None)
        } else {
            Ok(Some(Self {
                key: format!("{}-page-1", self.key),
            }))
        }
    }
}

api! {
    client PaginationKeyLimitApi {
        base "http://pagination-key-limit.example.test/v1/";
        GET Items(key: String) -> crate::KeyPageCodec {
            path ["items"];
            query { "cursor" = ep.key; }
            rate_limit { 1 / 1m; }
            paginate crate::MultiPageKeyController {
                key: bind ep.key,
            }
        }
    }
}

#[derive(Default)]
struct KeyLimitHooks {
    before_calls: AtomicUsize,
    safe: Mutex<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PageCapture {
    page_index: u32,
    rules: Vec<(
        String,
        Vec<(String, String)>,
        u32,
        Vec<(u32, std::time::Duration)>,
    )>,
}

#[derive(Default)]
struct PaginationCapturingLimiter {
    acquisitions: Mutex<Vec<PageCapture>>,
}

impl PaginationCapturingLimiter {
    fn capture(request: &concord_core::advanced::RateLimitRequest<'_>) -> PageCapture {
        PageCapture {
            page_index: request.page_index,
            rules: request
                .plan
                .rules()
                .iter()
                .map(|rule| {
                    (
                        rule.id().as_str().to_owned(),
                        rule.key()
                            .parts()
                            .iter()
                            .map(|part| (part.name().to_owned(), part.value().to_owned()))
                            .collect(),
                        rule.cost().get(),
                        rule.windows()
                            .iter()
                            .map(|window| (window.capacity().get(), window.duration()))
                            .collect(),
                    )
                })
                .collect(),
        }
    }
}

impl RateLimiter for PaginationCapturingLimiter {
    fn acquire<'a>(
        &'a self,
        request: concord_core::advanced::RateLimitRequest<'a>,
    ) -> BoxFuture<
        'a,
        Result<concord_core::advanced::RateLimitPermit, concord_core::advanced::RateLimitError>,
    > {
        let capture = Self::capture(&request);
        Box::pin(async move {
            self.acquisitions
                .lock()
                .expect("pagination capture lock")
                .push(capture);
            Ok(concord_core::advanced::RateLimitPermit::granted())
        })
    }

    fn observe<'a>(
        &'a self,
        _response: concord_core::advanced::RateLimitResponse<'a>,
    ) -> BoxFuture<'a, ()> {
        Box::pin(async {})
    }
}

impl RuntimeHooks for KeyLimitHooks {
    fn before_send<'a>(
        &'a self,
        context: concord_core::advanced::BeforeSend<'a>,
    ) -> BoxFuture<'a, Result<(), HookError>> {
        self.before_calls.fetch_add(1, Ordering::SeqCst);
        self.safe
            .lock()
            .expect("safe hook capture")
            .push(format!("{} {:?}", context.safe_url, context.headers));
        Box::pin(async { Ok(()) })
    }
}

#[tokio::test]
async fn generated_pagination_uses_a_value_neutral_global_bucket() {
    use pagination_key_limit_api::prelude::*;

    const BOUNDARY: &str = "generated-page-key-boundary-sentinel-c7105e";
    let key_limit = 1;

    let accepted_server =
        ScriptedServer::start([ResponseScript::ok(b"1".to_vec())]).expect("accepted server");
    let accepted_proxy = SafeProxy::parse(accepted_server.url("")).expect("accepted proxy");
    let accepted_hooks = Arc::new(KeyLimitHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = accepted_hooks.clone();
    let accepted_limiter: Arc<dyn RateLimiter> = Arc::new(
        GovernorRateLimiter::new()
            .key_limit(key_limit)
            .expect("accepted key limit"),
    );
    let accepted_client = PaginationKeyLimitApi::builder()
        .http(move |http| http.proxy(accepted_proxy))
        .runtime(move |runtime| {
            runtime
                .hooks(configured_hooks)
                .rate_limiter(accepted_limiter)
        })
        .build()
        .expect("accepted key client");
    let items = accepted_client
        .items(BOUNDARY.to_owned())
        .paginate(1_u32)
        .await
        .expect("global generated key");
    assert_eq!(items, [1]);
    assert_eq!(accepted_hooks.before_calls.load(Ordering::SeqCst), 1);
    assert_eq!(accepted_server.requests().len(), 1);
    assert!(accepted_server.requests()[0].target().contains(BOUNDARY));
    assert!(
        accepted_hooks
            .safe
            .lock()
            .expect("accepted safe capture")
            .iter()
            .all(|safe| !safe.contains(BOUNDARY))
    );

    let excess = format!("{BOUNDARY}x");
    let excess_server =
        ScriptedServer::start([ResponseScript::ok(b"2".to_vec())]).expect("excess server");
    let excess_proxy = SafeProxy::parse(excess_server.url("")).expect("excess proxy");
    let excess_hooks = Arc::new(KeyLimitHooks::default());
    let configured_hooks: Arc<dyn RuntimeHooks> = excess_hooks.clone();
    let excess_limiter: Arc<dyn RateLimiter> = Arc::new(
        GovernorRateLimiter::new()
            .key_limit(key_limit)
            .expect("excess key limit"),
    );
    let excess_client = PaginationKeyLimitApi::builder()
        .http(move |http| http.proxy(excess_proxy))
        .runtime(move |runtime| runtime.hooks(configured_hooks).rate_limiter(excess_limiter))
        .build()
        .expect("excess key client");
    let items = excess_client
        .items(excess.clone())
        .paginate(1_u32)
        .await
        .expect("request values do not enter generated keys");
    assert_eq!(items, [2]);
    assert_eq!(excess_hooks.before_calls.load(Ordering::SeqCst), 1);
    assert_eq!(excess_server.requests().len(), 1);
    assert!(excess_server.requests()[0].target().contains(&excess));
}

#[tokio::test]
async fn pagination_reuses_identical_lexical_plan_across_multiple_pages() {
    use pagination_key_limit_api::prelude::*;

    const BOUNDARY: &str = "pagination-continuation-sentinel-2fb8";
    let server = ScriptedServer::start([
        ResponseScript::ok(b"10".to_vec()),
        ResponseScript::ok(b"20".to_vec()),
    ])
    .expect("multi-page capture server");
    let proxy = SafeProxy::parse(server.url("")).expect("multi-page capture proxy");
    let limiter = Arc::new(PaginationCapturingLimiter::default());
    let client = PaginationKeyLimitApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("multi-page capture client");

    let items = client
        .items(BOUNDARY.to_owned())
        .paginate(2_u32)
        .await
        .expect("two-page capture");
    assert_eq!(items, [10, 20]);
    let captures = limiter.acquisitions.lock().expect("page captures");
    assert_eq!(
        captures
            .iter()
            .map(|capture| capture.page_index)
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(captures[0].rules, captures[1].rules);
    assert_eq!(captures[0].rules.len(), 1);
    assert!(captures[0].rules[0].1.is_empty());
    assert_eq!(captures[0].rules[0].2, 1);
    assert_eq!(
        captures[0].rules[0].3,
        [(1, std::time::Duration::from_secs(60))]
    );
    assert!(server.requests()[0].target().contains(BOUNDARY));
    assert!(
        server.requests()[1]
            .target()
            .contains(&format!("{BOUNDARY}-page-1"))
    );
}

#[tokio::test]
async fn pagination_pages_share_the_same_lexical_governor_bucket() {
    use pagination_key_limit_api::prelude::*;

    const BOUNDARY: &str = "pagination-governor-sentinel-91c2";
    let server = ScriptedServer::start([ResponseScript::ok(b"10".to_vec())])
        .expect("pagination governor server");
    let proxy = SafeProxy::parse(server.url("")).expect("pagination governor proxy");
    let limiter: Arc<dyn RateLimiter> = Arc::new(
        GovernorRateLimiter::new()
            .key_limit(64)
            .expect("bounded governor key limit"),
    );
    let client = PaginationKeyLimitApi::builder()
        .http(move |http| http.proxy(proxy))
        .runtime({
            let limiter = Arc::clone(&limiter);
            move |runtime| runtime.rate_limiter(limiter)
        })
        .build()
        .expect("pagination governor client");
    let operation =
        tokio::spawn(async move { client.items(BOUNDARY.to_owned()).paginate(2_u32).await });
    for _ in 0..32 {
        tokio::task::yield_now().await;
    }
    server
        .wait_for_requests(1, std::time::Duration::from_secs(1))
        .expect("first page reached server");
    tokio::task::yield_now().await;
    assert_eq!(server.request_count(), 1);
    operation.abort();
    assert!(operation.await.is_err());
}
