use concord_core::prelude::{RetryMode, Text};
use concord_macros::api;

use self::dynamic_construction_api::DynamicConstructionApi;
use self::fixed_http_construction_api::FixedHttpConstructionApi;
use self::fixed_https_construction_api::FixedHttpsConstructionApi;

api! {
    client FixedHttpConstructionApi { base "http://example.test" }
    GET Ping path ["ping"] -> Text<String>
}

api! {
    client FixedHttpsConstructionApi { base "https://example.test" }
    GET Ping path ["ping"] -> Text<String>
}

api! {
    client DynamicConstructionApi {
        base "http://example.test"
        var authority: String
    }
    scope runtime {
        host [vars.authority]
        GET Ping path ["ping"] -> Text<String>
    }
}

#[test]
fn generated_fallible_builders_accept_http_and_tls_enabled_https() {
    FixedHttpConstructionApi::builder()
        .build()
        .expect("fixed HTTP construction");
    FixedHttpsConstructionApi::builder()
        .build()
        .expect("TLS-enabled fixed HTTPS construction");
    DynamicConstructionApi::builder()
        .authority("runtime.example.test".to_string())
        .build()
        .expect("dynamic construction defers URL capability validation");
}

#[test]
fn every_generated_fallible_constructor_accepts_tls_enabled_fixed_https() {
    FixedHttpsConstructionApi::new_with_safe_reqwest_builder(|builder| builder)
        .expect("safe builder construction");
    FixedHttpsConstructionApi::new_with_safe_reqwest_builder_fallible(Ok)
        .expect("fallible safe builder construction");
    FixedHttpsConstructionApi::new_with_retry_mode(RetryMode::Disabled)
        .expect("retry-mode construction");
    FixedHttpsConstructionApi::new_with_safe_reqwest_builder_and_retry_mode(
        RetryMode::Disabled,
        Ok,
    )
    .expect("fallible retry-mode construction");
}
