use std::time::Duration;

use concord_core::advanced::{HttpConfigBuilder, ProviderRetry, SafeProxy};

fn always_available_methods(builder: HttpConfigBuilder, proxy: SafeProxy) -> HttpConfigBuilder {
    builder
        .connect_timeout(Duration::from_secs(1))
        .read_timeout(Duration::from_secs(2))
        .pool_idle_timeout(Duration::from_secs(3))
        .pool_max_idle_per_host(4)
        .tcp_keepalive(Duration::from_secs(5))
        .tcp_nodelay(true)
        .https_only(true)
        .http1_only()
        .proxy(proxy)
        .retry_reqwest_default()
        .retry_off()
        .provider_retry(ProviderRetry::Disabled)
}

fn assert_value_traits<T: Clone + Copy + std::fmt::Debug + Eq + PartialEq>() {}

#[test]
fn downstream_http_configuration_has_final_always_available_signatures() {
    let _: fn(HttpConfigBuilder, SafeProxy) -> HttpConfigBuilder = always_available_methods;
    assert_value_traits::<ProviderRetry>();
    assert_ne!(ProviderRetry::ProtocolRecovery, ProviderRetry::Disabled);
}
