#![forbid(unsafe_code)]

use std::any::type_name;

use concord::{
    auth, body, codec, endpoint, error, hooks, http, pagination, rate_limit, response, runtime,
};

const _: () = assert!(
    endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES <= endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES
);
const _: () =
    assert!(rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES <= rate_limit::MAX_RATE_LIMIT_KEY_BYTES);

fn public_type<T>() -> &'static str {
    type_name::<T>()
}

#[test]
fn semantic_domains_are_available_without_implementation_crate_imports() {
    let names = [
        public_type::<auth::Auth>(),
        public_type::<body::RequestBody>(),
        public_type::<codec::Text>(),
        public_type::<endpoint::RuntimeEndpoint>(),
        public_type::<error::ApiError>(),
        public_type::<error::ClientInputKind>(),
        public_type::<hooks::DebugLevel>(),
        public_type::<http::Method>(),
        public_type::<pagination::OffsetPagination>(),
        public_type::<rate_limit::NoopRateLimiter>(),
        public_type::<response::NoContent>(),
        public_type::<runtime::OperationId>(),
        public_type::<runtime::RuntimeConfigBuilder>(),
    ];

    assert!(names.iter().all(|name| !name.is_empty()));
}

#[test]
fn hard_structural_limits_are_reachable_from_their_facade_domains() {
    let auth_limits = [
        auth::MAX_AUTH_PLACEMENT_NAME_BYTES,
        auth::MAX_CREDENTIAL_SECRET_BYTES,
        auth::MAX_BASIC_USERNAME_BYTES,
        auth::MAX_BASIC_PASSWORD_BYTES,
        auth::MAX_AUTHORIZATION_VALUE_BYTES,
        auth::MAX_AUTH_QUERY_VALUE_BYTES,
    ];
    #[cfg(feature = "multipart")]
    let body_limits = [
        body::MAX_MULTIPART_PARTS,
        body::MAX_MULTIPART_NAME_BYTES,
        body::MAX_MULTIPART_FILENAME_BYTES,
        body::MAX_MULTIPART_CONTENT_TYPE_BYTES,
        body::MAX_MULTIPART_METADATA_BYTES,
        body::MAX_MULTIPART_INLINE_TEXT_BYTES,
    ];
    let endpoint_limits = [
        endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES,
        endpoint::MAX_RUNTIME_ENDPOINT_COMPONENTS,
        endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
        endpoint::MAX_RUNTIME_ENDPOINT_AUTH_PLACEMENTS,
    ];
    let rate_limits = [
        rate_limit::MAX_RATE_LIMIT_ID_BYTES,
        rate_limit::MAX_RATE_LIMIT_KEY_PARTS,
        rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
        rate_limit::MAX_RATE_LIMIT_KEY_BYTES,
        rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_RULE,
        rate_limit::MAX_RATE_LIMIT_RULES_PER_PLAN,
    ];

    assert!(auth_limits.into_iter().all(|limit| limit > 0));
    #[cfg(feature = "multipart")]
    assert!(body_limits.into_iter().all(|limit| limit > 0));
    assert!(endpoint_limits.into_iter().all(|limit| limit > 0));
    assert!(rate_limits.into_iter().all(|limit| limit > 0));
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
    assert_eq!(concord::auth::MAX_OAUTH2_TOKEN_URL_BYTES, 8_192);
    assert_eq!(concord::auth::MAX_OAUTH2_CLIENT_ID_BYTES, 8_192);
    assert_eq!(concord::auth::MAX_OAUTH2_CLIENT_SECRET_BYTES, 8_192);
    assert_eq!(concord::auth::MAX_OAUTH2_SCOPE_BYTES, 65_536);
    assert_eq!(concord::auth::MAX_OAUTH2_AUTHORIZATION_HEADER_BYTES, 32_768);
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
