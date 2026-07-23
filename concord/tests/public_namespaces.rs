#![forbid(unsafe_code)]

use std::any::type_name;

use concord::{
    auth, body, codec, endpoint, error, hooks, http, pagination, rate_limit, response, runtime,
};

fn public_type<T>() -> &'static str {
    type_name::<T>()
}

#[test]
fn semantic_domains_are_available_without_implementation_crate_imports() {
    let names = [
        public_type::<auth::Auth>(),
        public_type::<body::RequestBody>(),
        public_type::<codec::Text>(),
        public_type::<endpoint::PreparedEndpoint>(),
        public_type::<error::ApiError>(),
        public_type::<hooks::DebugLevel>(),
        public_type::<http::Method>(),
        public_type::<pagination::OffsetPagination>(),
        public_type::<rate_limit::NoopRateLimiter>(),
        public_type::<response::NoContent>(),
        public_type::<runtime::OperationId>(),
    ];

    assert!(names.iter().all(|name| !name.is_empty()));
}

#[cfg(feature = "records")]
#[test]
fn records_contracts_are_exposed_by_the_records_domain() {
    assert!(!public_type::<concord::records::RecordError>().is_empty());
}

#[cfg(feature = "json")]
#[test]
fn json_is_exposed_by_the_codec_domain_and_curated_top_level() {
    assert_eq!(
        public_type::<concord::codec::Json<Vec<u32>>>(),
        public_type::<concord::Json<Vec<u32>>>()
    );
}

#[cfg(feature = "multipart")]
#[test]
fn multipart_remains_core_owned_but_is_exposed_by_the_body_domain() {
    assert_eq!(
        public_type::<concord::body::Multipart>(),
        public_type::<concord::Multipart>()
    );
}

#[cfg(feature = "oauth2-client")]
#[test]
fn oauth_provider_is_exposed_by_the_auth_domain() {
    assert!(!public_type::<concord::auth::OAuth2ClientCredentials>().is_empty());
}

#[cfg(feature = "rate-limit-governor")]
#[test]
fn governor_is_exposed_by_the_rate_limit_domain() {
    assert!(!public_type::<concord::rate_limit::GovernorRateLimiter>().is_empty());
}

#[cfg(feature = "dangerous-raw-response")]
#[test]
fn dangerous_raw_response_requires_the_explicit_domain_and_feature() {
    assert!(!public_type::<concord::dangerous::RawResponse>().is_empty());
}

#[cfg(feature = "dangerous-custom-retry")]
#[test]
fn dangerous_retry_requires_the_explicit_domain_and_feature() {
    assert!(!public_type::<concord::dangerous::ReqwestRetryBuilder>().is_empty());
}
