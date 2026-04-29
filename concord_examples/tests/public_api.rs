#![allow(unused_imports)]
use concord_core::advanced::{
    BuiltRequest, BuiltResponse, CacheConfig, CacheFuture, CacheStore, CredentialMaterial,
    CredentialProvider, RateLimitFuture, RateLimitPermit, RateLimitPlan, RateLimiter, RetryPolicy,
    RuntimeHooks, Transport, TransportError, TransportResponse,
};
use concord_core::prelude::{
    AccessToken, ApiClient, ApiClientError, ApiKey, BasicCredential, ClientContext, DebugLevel,
    Endpoint, Json, NoContent, PaginatedRequest, PendingRequest, RateLimitObservation,
    RateLimitObserver, RateLimitResponseContext, SecretString, Text,
};

#[test]
fn prelude_and_advanced_exports_are_intentional() {
    let _ = core::any::type_name::<AccessToken>();
    let _ = core::any::type_name::<ApiClientError>();
    let _ = core::any::type_name::<ApiKey>();
    let _ = core::any::type_name::<BasicCredential>();
    let _ = core::any::type_name::<DebugLevel>();
    let _ = core::any::type_name::<Json>();
    let _ = core::any::type_name::<NoContent>();
    let _ = core::any::type_name::<RateLimitObservation>();
    let _ = core::any::type_name::<RateLimitResponseContext<'_>>();
    let _ = core::any::type_name::<SecretString>();
    let _ = core::any::type_name::<Text>();

    let _ = core::any::type_name::<CacheConfig>();
    let _ = core::any::type_name::<BuiltRequest>();
    let _ = core::any::type_name::<BuiltResponse>();
    let _ = core::any::type_name::<CacheFuture<'_, ()>>();
    let _ = core::any::type_name::<RateLimitPermit>();
    let _ = core::any::type_name::<RateLimitFuture<'_, ()>>();
    let _ = core::any::type_name::<RateLimitPlan>();
    let _ = core::any::type_name::<TransportError>();
    let _ = core::any::type_name::<TransportResponse>();

    fn _prelude_traits<Cx, E, T>()
    where
        Cx: ClientContext,
        E: Endpoint<Cx>,
        T: Transport,
    {
        let _ = core::any::type_name::<ApiClient<Cx, T>>();
        let _ = core::any::type_name::<PendingRequest<'_, Cx, E, T>>();
        let _ = core::any::type_name::<PaginatedRequest<'_, Cx, E, T>>();
    }

    fn _advanced_traits<T, C, R, H, P>()
    where
        T: Transport,
        C: CacheStore,
        R: RateLimiter,
        H: RuntimeHooks,
        P: RetryPolicy,
    {
    }

    fn _credential_traits<Cx, P>()
    where
        Cx: ClientContext,
        P: CredentialProvider<Cx>,
        P::Credential: CredentialMaterial,
    {
    }

    fn _observer<O: RateLimitObserver>() {}
}
