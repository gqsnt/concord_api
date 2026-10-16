use std::time::Duration;

use concord_core::contracts::{RetryDecision, RetryPolicy, RuntimeConfigBuilder};

// §20.4: the ordinary count/size setters take primitive values. Zero is
// recorded as invalid configuration and reported by the
// root build rather than being unrepresentable in the signature; the core
// unit test `limit_setters_take_primitives_and_report_invalid_latest_selection`
// proves that latch.
fn finite_methods(builder: RuntimeConfigBuilder, bytes: usize, pages: u32) -> RuntimeConfigBuilder {
    builder
        .request_body_limit(bytes)
        .response_body_limit(bytes)
        .stream_response_limit(bytes)
        .stream_lifetime(Duration::from_secs(300))
        .stream_idle_timeout(Duration::from_secs(30))
        .stream_max_records(bytes)
        .stream_frame_limit(bytes)
        .status_body_limit(bytes)
        .disable_status_body_capture()
        .pagination_max_pages(pages)
        .pagination_max_items(bytes)
        .pagination_total_response_body_limit(bytes)
        .pagination_identity_limit(bytes)
        .aggregate_max_sends(pages)
        .auth_failure_cooldown_cap(Duration::ZERO)
        .rate_limit_cooldown_cap(Duration::ZERO)
        .provider_request_limit(bytes)
        .provider_response_limit(bytes)
        .provider_max_sends(4)
        .provider_max_origins(8)
        .provider_timeout(Duration::from_secs(30))
}

#[test]
fn downstream_runtime_configuration_takes_primitive_ordinary_limits() {
    let _: fn(RuntimeConfigBuilder, usize, u32) -> RuntimeConfigBuilder = finite_methods;
}

#[test]
fn downstream_runtime_configuration_selects_or_disables_managed_retry() {
    fn retry_methods(builder: RuntimeConfigBuilder, policy: RetryPolicy) -> RuntimeConfigBuilder {
        builder.retry_policy(policy).disable_retry_policy()
    }

    let _: fn(RuntimeConfigBuilder, RetryPolicy) -> RuntimeConfigBuilder = retry_methods;
    let _ = RetryPolicy::builder()
        .without_budget()
        .classify_fn(|_| RetryDecision::Stop)
        .build()
        .expect("retry policy");
}
