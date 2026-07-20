#![cfg(feature = "rate-limit-governor")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use concord_core::advanced::{
    BoxFuture, CodecError, DecodeBody, DecodeContext, ErrorCategory, GovernorRateLimiter,
    HookError, PaginationController, PaginationError, RateLimiter, RuntimeHooks, SafeProxy,
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

pub struct OnePageKeyController {
    pub key: String,
}

impl PaginationController<Vec<u32>> for OnePageKeyController {
    fn next(self, _page: &Vec<u32>) -> Result<Option<Self>, PaginationError> {
        Ok(None)
    }
}

api! {
    client PaginationKeyLimitApi {
        base "http://pagination-key-limit.example.test/v1/";
        rate_limits {
            page_key {
                by [ep.key];
                1 / 1m;
            }
        }
        GET Items(key: String) -> crate::KeyPageCodec {
            path ["items"];
            query { "cursor" = ep.key; }
            rate_limit page_key;
            paginate crate::OnePageKeyController {
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
async fn generated_pagination_key_limit_measures_actual_value_not_placeholder_text() {
    use pagination_key_limit_api::prelude::*;

    const BOUNDARY: &str = "generated-page-key-boundary-sentinel-c7105e";
    const KEY_NAME: &str = "ep.key";
    let key_limit = KEY_NAME.len() + BOUNDARY.len();

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
        .expect("actual boundary key");
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
    let error = excess_client
        .items(excess.clone())
        .paginate(1_u32)
        .await
        .expect_err("first actual byte above the generated key limit");
    assert_eq!(error.category(), ErrorCategory::Configuration);
    assert_eq!(excess_hooks.before_calls.load(Ordering::SeqCst), 0);
    assert!(excess_hooks.safe.lock().expect("excess safe").is_empty());
    assert!(excess_server.requests().is_empty());

    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(&error);
    while let Some(current) = source {
        assert!(!current.to_string().contains(&excess));
        assert!(!format!("{current:?}").contains(&excess));
        source = current.source();
    }
}
