#[cfg(not(feature = "dangerous-custom-retry"))]
#[test]
fn compiled_default_surface_enables_no_dangerous_feature() {
    assert!(!cfg!(feature = "dangerous-custom-retry"));
}

#[cfg(feature = "dangerous-custom-retry")]
#[test]
fn custom_retry_is_available_without_other_optional_features() {
    use concord_core::contracts::HttpConfigBuilder;
    use concord_core::dangerous::ReqwestRetryBuilder;

    fn configure(builder: HttpConfigBuilder, retry: ReqwestRetryBuilder) -> HttpConfigBuilder {
        builder.dangerous_custom_retry(retry)
    }

    let _: fn(HttpConfigBuilder, ReqwestRetryBuilder) -> HttpConfigBuilder = configure;
}
