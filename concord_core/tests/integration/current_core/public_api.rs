use concord_core::{advanced, prelude};
use std::sync::Arc;

#[test]
fn public_v1_surface_compiles() {
    fn uses_type<T>() {}
    fn uses_endpoint<E: prelude::Endpoint<super::common::TestCx>>() {}
    fn uses_client_context<Cx: prelude::ClientContext>() {}
    fn uses_standard_body<B: http_body::Body<Data = bytes::Bytes, Error = advanced::BodyError>>() {}
    fn uses_rate_limiter<L: advanced::RateLimiter>() {}
    fn uses_hooks<H: advanced::RuntimeHooks>() {}
    fn uses_debug_sink<S: advanced::DebugSink>() {}
    fn uses_page_items<P: prelude::PageItems>() {}
    fn uses_next_cursor<P: prelude::HasNextCursor>() {}

    uses_type::<prelude::ApiClient<super::common::TestCx>>();
    uses_type::<prelude::ApiClientError>();
    uses_type::<advanced::RuntimeConfig>();
    uses_type::<prelude::RetryMode>();
    uses_type::<prelude::StatusRetryConfig>();
    uses_type::<prelude::DebugLevel>();
    uses_endpoint::<super::common::TextEndpoint>();
    uses_client_context::<super::common::TestCx>();
    uses_standard_body::<advanced::DynBody>();
    uses_rate_limiter::<advanced::NoopRateLimiter>();
    uses_hooks::<advanced::NoopRuntimeHooks>();
    uses_debug_sink::<advanced::NoopDebugSink>();
    uses_page_items::<Vec<String>>();
    uses_next_cursor::<Vec<String>>();

    uses_type::<http::Response<advanced::DynBody>>();
    uses_type::<advanced::TransportError>();
    uses_type::<advanced::SafeProxyError>();
    uses_type::<advanced::TransportErrorKind>();
    uses_type::<advanced::StreamBody>();
    uses_type::<advanced::StreamBodyError>();
    uses_type::<http_body::SizeHint>();
    uses_type::<advanced::DynBody>();
    uses_type::<advanced::BodyError>();
    uses_type::<advanced::BodyErrorKind>();
    uses_type::<advanced::LimitedBody<advanced::DynBody>>();
    uses_type::<advanced::OctetStream>();
    uses_type::<advanced::Mp3>();
    uses_type::<advanced::Mp4>();
    uses_type::<advanced::Pdf>();
    uses_type::<advanced::Zip>();
    uses_type::<advanced::Png>();
    uses_type::<advanced::Jpeg>();
    uses_type::<advanced::RateLimitContext<'static>>();
    uses_type::<advanced::RateLimitPermit>();
    uses_type::<advanced::RateLimitResponseContext<'static>>();
    uses_type::<advanced::RateLimitResponseAction>();
    uses_type::<advanced::AuthPlacement>();
    uses_type::<advanced::AuthDecision>();
    uses_type::<advanced::AuthError>();
    uses_type::<prelude::PaginationTermination>();
    uses_type::<concord_core::internal::ResolvedPolicy>();

    #[allow(clippy::let_unit_value)]
    let _safe_proxy_error_variants = (
        advanced::SafeProxyError::InvalidOrigin,
        advanced::SafeProxyError::TlsUnavailable,
    );
}

#[test]
fn prelude_surface_contains_normal_user_api() {
    fn assert_endpoint<E: prelude::Endpoint<super::common::TestCx>>() {}
    assert_endpoint::<super::common::TextEndpoint>();

    let _debug = prelude::DebugLevel::default();
    let _secret = prelude::SecretString::new("redacted");
    let _api_key = prelude::ApiKey::new("key");
    let _basic = prelude::BasicCredential::new("user", "pass");
}

#[test]
fn advanced_surface_contains_extension_api() {
    let mut cfg = advanced::RuntimeConfig::default();
    cfg.rate_limiter(Arc::new(advanced::NoopRateLimiter::new()));
    cfg.runtime_hooks(Arc::new(advanced::NoopRuntimeHooks));

    let status = prelude::StatusRetryConfig::new(2, [http::StatusCode::BAD_GATEWAY])
        .expect("approved status retry config");
    let _modes = [
        prelude::RetryMode::ProtocolRecovery,
        prelude::RetryMode::Disabled,
        prelude::RetryMode::Status(status),
    ];

    let _ctx_ty: Option<advanced::RateLimitResponseContext<'_>> = None;
    let _slot_ty: Option<
        advanced::CredentialSlot<super::common::TestCx, advanced::StaticBearerProvider>,
    > = None;
}
