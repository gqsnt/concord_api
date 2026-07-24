use std::time::Duration;

use concord_core::contracts::RuntimeConfigBuilder;

#[test]
fn runtime_endpoint_timeout_names_expose_their_exact_scope() {
    let _: fn(RuntimeConfigBuilder, Duration) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::runtime_endpoint_timeout;
    let _: fn(RuntimeConfigBuilder) -> RuntimeConfigBuilder =
        RuntimeConfigBuilder::disable_runtime_endpoint_timeout;
}
