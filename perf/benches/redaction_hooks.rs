use bytes::Bytes;
use concord_core::advanced::{
    DebugSink, NoopRateLimiter, PostResponseHookContext, PreSendHookContext, RateLimiter,
    RuntimeHooks, SanitizedHeaders,
};
use concord_core::internal::{PreparedBody, RequestOverrides, RequestPlan, ResolvedPolicy};
use concord_core::prelude::{ApiClientError, DebugLevel};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use http::{HeaderName, HeaderValue, Method, StatusCode};
use perf::support::{MockResponse, MockTransport, client, configured_client, execute_raw_plan, request_plan, runtime};
use std::future::Future;
use std::hint::black_box;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Default)]
struct CountingDebugSink {
    count: AtomicUsize,
}

impl CountingDebugSink {
    fn observe_headers(&self, headers: SanitizedHeaders<'_>) {
        let mut seen = 0usize;
        for (_name, value) in headers.iter() {
            seen += value.as_str().len();
        }
        self.count.fetch_add(seen, Ordering::Relaxed);
    }
}

impl DebugSink for CountingDebugSink {
    fn request_start(
        &self,
        _dbg: DebugLevel,
        _method: &Method,
        url: &str,
        _endpoint: &'static str,
        _page_index: u32,
    ) {
        self.count.fetch_add(url.len(), Ordering::Relaxed);
    }

    fn request_headers(&self, _dbg: DebugLevel, headers: SanitizedHeaders<'_>) {
        self.observe_headers(headers);
    }

    fn response_status(&self, _dbg: DebugLevel, status: StatusCode, url: &str, _ok: bool) {
        self.count
            .fetch_add(status.as_u16() as usize + url.len(), Ordering::Relaxed);
    }

    fn response_headers(&self, _dbg: DebugLevel, headers: SanitizedHeaders<'_>) {
        self.observe_headers(headers);
    }
}

#[derive(Default)]
struct CountingHooks {
    count: AtomicUsize,
}

impl RuntimeHooks for CountingHooks {
    fn pre_send<'a>(
        &'a self,
        ctx: PreSendHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.count
                .fetch_add(ctx.meta.url.len() + ctx.headers.len(), Ordering::Relaxed);
            Ok(())
        })
    }

    fn post_response<'a>(
        &'a self,
        ctx: PostResponseHookContext<'a>,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.count
                .fetch_add(ctx.status.as_u16() as usize + ctx.headers.len(), Ordering::Relaxed);
        })
    }
}

fn plan(name: &'static str, query: usize, headers: usize, mixed_case: bool) -> RequestPlan {
    let mut policy = ResolvedPolicy::default();
    for idx in 0..query {
        let key = if idx % 2 == 0 {
            if mixed_case {
                format!("ToKeN_{idx}")
            } else {
                format!("token_{idx}")
            }
        } else {
            if mixed_case {
                format!("Visible_{idx}")
            } else {
                format!("visible_{idx}")
            }
        };
        policy
            .query
            .push((key, format!("BENCH_FAKE_QUERY_SECRET_{idx}")));
    }
    for idx in 0..headers {
        let name = if idx % 2 == 0 {
            if mixed_case {
                format!("X-Api-Token-{idx}")
            } else {
                format!("x-api-token-{idx}")
            }
        } else {
            if mixed_case {
                format!("X-Visible-{idx}")
            } else {
                format!("x-visible-{idx}")
            }
        };
        policy.headers.insert(
            HeaderName::from_bytes(name.as_bytes()).expect("valid header"),
            HeaderValue::from_str(&format!("BENCH_FAKE_HEADER_SECRET_{idx}"))
                .expect("valid header value"),
        );
    }
    request_plan(
        name,
        Method::GET,
        "/perf/redaction",
        policy,
        PreparedBody::empty(),
    )
}

fn response(headers: usize, mixed_case: bool) -> MockResponse {
    let mut response = MockResponse::text(StatusCode::OK, Bytes::from_static(b"ok"));
    for idx in 0..headers {
        let name = if idx % 2 == 0 {
            if mixed_case {
                format!("X-Refresh-Token-{idx}")
            } else {
                format!("x-refresh-token-{idx}")
            }
        } else {
            if mixed_case {
                format!("X-Response-Visible-{idx}")
            } else {
                format!("x-response-visible-{idx}")
            }
        };
        response = response.with_header(
            HeaderName::from_bytes(name.as_bytes()).expect("valid header"),
            HeaderValue::from_str(&format!("BENCH_FAKE_RESPONSE_SECRET_{idx}"))
                .expect("valid header value"),
        );
    }
    response
}

fn bench_case(c: &mut Criterion, name: &str, query: usize, req_headers: usize, resp_headers: usize, debug: DebugLevel, hooks: bool, mixed_case: bool) {
    let rt = runtime();
    c.bench_function(name, |b| {
        b.to_async(&rt).iter_batched(
            || {
                let transport = MockTransport::repeating(response(resp_headers, mixed_case));
                let mut client = if debug == DebugLevel::None {
                    client(transport)
                } else {
                    configured_client(
                        transport,
                        debug,
                        Arc::new(NoopRateLimiter::new()) as Arc<dyn RateLimiter>,
                    )
                };
                let sink = Arc::new(CountingDebugSink::default());
                client.set_debug_sink(sink.clone());
                if hooks {
                    client.set_runtime_hooks(Arc::new(CountingHooks::default()));
                }
                let mut plan = plan("RedactionHooks", query, req_headers, mixed_case);
                plan.overrides = RequestOverrides {
                    debug_level: Some(debug),
                    ..Default::default()
                };
                (client, plan, sink)
            },
            |(client, plan, sink)| async move {
                let response = execute_raw_plan(&client, plan)
                    .await
                    .expect("redaction hooks bench");
                black_box((response.status, sink.count.load(Ordering::Relaxed)));
            },
            BatchSize::SmallInput,
        )
    });
}

fn redaction_hooks(c: &mut Criterion) {
    bench_case(c, "url_query/small", 4, 0, 0, DebugLevel::V, false, false);
    bench_case(c, "url_query/many", 64, 0, 0, DebugLevel::V, false, false);
    bench_case(c, "headers/small", 0, 4, 4, DebugLevel::VV, false, false);
    bench_case(c, "headers/many", 0, 64, 64, DebugLevel::VV, false, false);
    bench_case(c, "headers/mixed_case", 0, 32, 32, DebugLevel::VV, false, true);
    bench_case(c, "debug/disabled", 8, 8, 8, DebugLevel::None, false, false);
    bench_case(c, "debug/v", 8, 8, 8, DebugLevel::V, false, false);
    bench_case(c, "debug/vv", 8, 8, 8, DebugLevel::VV, false, false);
    bench_case(c, "hooks/noop_runtime", 8, 8, 8, DebugLevel::None, false, false);
    bench_case(c, "hooks/counting_runtime", 8, 8, 8, DebugLevel::None, true, false);
    bench_case(c, "hooks_debug/real_path_vv", 16, 16, 16, DebugLevel::VV, true, true);
}

criterion_group!(benches, redaction_hooks);
criterion_main!(benches);
