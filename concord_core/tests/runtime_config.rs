use std::num::{NonZeroU32, NonZeroUsize};
use std::time::Duration;

use concord_core::advanced::RuntimeConfigBuilder;

fn finite_methods(
    builder: RuntimeConfigBuilder,
    bytes: NonZeroUsize,
    pages: NonZeroU32,
) -> RuntimeConfigBuilder {
    builder
        .request_body_limit(bytes)
        .response_body_limit(bytes)
        .stream_response_limit(bytes)
        .status_body_limit(bytes)
        .disable_status_body_capture()
        .pagination_max_pages(pages)
        .pagination_max_items(bytes)
        .pagination_identity_limit(bytes)
        .auth_failure_cooldown_cap(Duration::ZERO)
        .rate_limit_cooldown_cap(Duration::ZERO)
        .provider_request_limit(bytes)
        .provider_response_limit(bytes)
}

#[test]
fn downstream_runtime_configuration_requires_nonzero_ordinary_limits() {
    let _: fn(RuntimeConfigBuilder, NonZeroUsize, NonZeroU32) -> RuntimeConfigBuilder =
        finite_methods;
}
