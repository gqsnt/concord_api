use concord_core::{advanced, prelude};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

#[test]
fn public_v1_surface_compiles() {
    fn uses_type<T>() {}
    fn uses_endpoint<E: prelude::Endpoint<super::common::TestCx>>() {}
    fn uses_client_context<Cx: prelude::ClientContext>() {}
    fn uses_transport<T: advanced::Transport>() {}
    fn uses_body<B: advanced::TransportBody>() {}
    fn uses_standard_body<B: http_body::Body<Data = bytes::Bytes, Error = advanced::BodyError>>() {}
    fn uses_rate_limiter<L: advanced::RateLimiter>() {}
    fn uses_hooks<H: advanced::RuntimeHooks>() {}
    fn uses_debug_sink<S: advanced::DebugSink>() {}
    fn uses_page_items<P: prelude::PageItems>() {}
    fn uses_next_cursor<P: prelude::HasNextCursor>() {}

    uses_type::<prelude::ApiClient<super::common::TestCx, super::common::MockTransport>>();
    uses_type::<prelude::ApiClientError>();
    uses_type::<advanced::RuntimeConfig>();
    uses_type::<prelude::DebugLevel>();
    uses_endpoint::<super::common::TextEndpoint>();
    uses_client_context::<super::common::TestCx>();
    uses_transport::<super::common::MockTransport>();
    uses_body::<EmptyBody>();
    uses_standard_body::<advanced::DynBody>();
    uses_rate_limiter::<advanced::NoopRateLimiter>();
    uses_hooks::<advanced::NoopRuntimeHooks>();
    uses_debug_sink::<advanced::NoopDebugSink>();
    uses_page_items::<Vec<String>>();
    uses_next_cursor::<Vec<String>>();

    uses_type::<advanced::TransportRequest>();
    uses_type::<advanced::TransportRequestBody>();
    uses_type::<advanced::TransportByteStream>();
    uses_type::<advanced::TransportResponse>();
    uses_type::<advanced::TransportError>();
    uses_type::<advanced::TransportErrorKind>();
    uses_type::<advanced::StreamBody>();
    uses_type::<advanced::StreamBodyError>();
    uses_type::<advanced::BodySizeHint>();
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
}

struct EmptyBody;

impl advanced::TransportBody for EmptyBody {
    fn next_chunk<'a>(
        &'a mut self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Option<bytes::Bytes>, advanced::TransportError>> + Send + 'a,
        >,
    > {
        Box::pin(async { Ok(None) })
    }
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
    cfg.retry_policy(Arc::new(advanced::ConfiguredRetryPolicy::new(
        advanced::RetryConfig {
            max_attempts: 1,
            methods: Vec::new(),
            statuses: Vec::new(),
            transport_errors: Vec::new(),
            backoff: advanced::RetryBackoff::None,
            respect_retry_after: false,
            idempotency: advanced::RetryIdempotency::SafeMethodsOnly,
        },
    )));
    cfg.runtime_hooks(Arc::new(advanced::NoopRuntimeHooks));
    cfg.max_auth_retries(2);

    let _ctx_ty: Option<advanced::RateLimitResponseContext<'_>> = None;
    let _slot_ty: Option<
        advanced::CredentialSlot<super::common::TestCx, advanced::StaticBearerProvider>,
    > = None;
}
