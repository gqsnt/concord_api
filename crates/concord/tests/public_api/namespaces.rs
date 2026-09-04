#![forbid(unsafe_code)]

use std::any::type_name;

use concord::{
    auth, body, codec, endpoint, error, http, observation, pagination, rate_limit, response, retry,
    runtime,
};

const _: () = assert!(
    endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES <= endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES
);
const _: () =
    assert!(rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES <= rate_limit::MAX_RATE_LIMIT_KEY_BYTES);

fn public_type<T>() -> &'static str {
    type_name::<T>()
}

#[cfg(feature = "records")]
struct NonCloneRecord;

#[cfg(feature = "records")]
struct PublicRecordDecoder;

#[cfg(feature = "records")]
impl concord::records::RecordDecoder for PublicRecordDecoder {
    type Record = NonCloneRecord;

    fn start(_head: &response::ResponseHead<'_>) -> Result<Self, concord::records::RecordError> {
        Ok(Self)
    }

    fn decode(
        &mut self,
        buffer: &mut concord::codec::BytesMut,
    ) -> Result<Option<Self::Record>, concord::records::RecordError> {
        Ok((!buffer.is_empty()).then(|| {
            drop(buffer.split_to(1));
            NonCloneRecord
        }))
    }

    fn decode_eof(
        &mut self,
        buffer: &mut concord::codec::BytesMut,
    ) -> Result<Option<Self::Record>, concord::records::RecordError> {
        self.decode(buffer)
    }

    fn max_buffered_bytes() -> std::num::NonZeroUsize {
        std::num::NonZeroUsize::new(8).expect("positive public decoder staging bound")
    }
}

#[test]
fn semantic_domains_are_available_without_implementation_crate_imports() {
    let names = [
        public_type::<auth::Auth>(),
        public_type::<body::RequestBody>(),
        public_type::<codec::CodecErrorKind>(),
        public_type::<codec::Text>(),
        public_type::<endpoint::RuntimeEndpoint>(),
        public_type::<endpoint::Request<()>>(),
        public_type::<endpoint::AcceptedBytesRequest<()>>(),
        public_type::<endpoint::ResponseRequest<()>>(),
        public_type::<error::ApiError>(),
        public_type::<error::ClientInputKind>(),
        public_type::<observation::AttemptInfo>(),
        public_type::<observation::DebugLevel>(),
        public_type::<http::Method>(),
        public_type::<pagination::OffsetPagination>(),
        public_type::<pagination::PaginatedRequest<()>>(),
        public_type::<rate_limit::NoopRateLimiter>(),
        public_type::<response::Empty>(),
        public_type::<retry::RetryPolicy>(),
        public_type::<retry::RetryAttempt<'static>>(),
        public_type::<retry::RetryFailure>(),
        public_type::<runtime::OperationId>(),
        public_type::<runtime::RuntimeConfigBuilder>(),
    ];

    assert!(names.iter().all(|name| !name.is_empty()));

    fn supports_accepted_bytes<T: response::AcceptedBytesDriver>() {}
    supports_accepted_bytes::<response::Buffered>();
    supports_accepted_bytes::<response::DiscardDriver>();

    fn supports_page_body<B, Input>()
    where
        B: body::PageRequestRepresentation<Input>,
        Input: Clone + Send + 'static,
    {
    }
    supports_page_body::<codec::Binary, concord::Bytes>();
    supports_page_body::<codec::Text, String>();
    #[cfg(feature = "json")]
    supports_page_body::<codec::Json, Vec<u32>>();
}

#[test]
fn hard_structural_limits_are_reachable_from_their_facade_domains() {
    let auth_limits = [
        auth::MAX_AUTH_PLACEMENT_NAME_BYTES,
        auth::MAX_AUTH_PLACEMENTS,
        auth::MAX_CREDENTIAL_SECRET_BYTES,
        auth::MAX_BASIC_USERNAME_BYTES,
        auth::MAX_BASIC_PASSWORD_BYTES,
        auth::MAX_AUTHORIZATION_SCHEME_BYTES,
        auth::MAX_AUTHORIZATION_VALUE_BYTES,
        auth::MAX_AUTH_QUERY_VALUE_BYTES,
    ];
    let body_limits = [body::MAX_EMPTY_REQUEST_CHUNKS];
    #[cfg(feature = "multipart")]
    let multipart_limits = [
        body::MAX_MULTIPART_PARTS,
        body::MAX_MULTIPART_NAME_BYTES,
        body::MAX_MULTIPART_FILENAME_BYTES,
        body::MAX_MULTIPART_CONTENT_TYPE_BYTES,
        body::MAX_MULTIPART_METADATA_BYTES,
        body::MAX_MULTIPART_INLINE_TEXT_BYTES,
    ];
    let endpoint_limits = [
        endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES,
        endpoint::MAX_REQUEST_HEAD_COMPONENTS,
        endpoint::MAX_REQUEST_URL_BYTES,
        endpoint::MAX_REQUEST_HEADER_COUNT,
        endpoint::MAX_REQUEST_HEADER_BYTES,
        endpoint::MAX_RUNTIME_ENDPOINT_COMPONENTS,
        endpoint::MAX_RUNTIME_ENDPOINT_METHOD_BYTES,
        endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES,
    ];
    let http_limits = [
        http::MAX_PROXY_URL_BYTES,
        http::MAX_ROOT_CERTIFICATES,
        http::MAX_ROOT_CERTIFICATE_BYTES,
        http::MAX_ROOT_CERTIFICATES_TOTAL_BYTES,
        http::MAX_IDENTITY_PEM_BYTES,
    ];
    let rate_limits = [
        rate_limit::MAX_RATE_LIMIT_ID_BYTES,
        rate_limit::MAX_RATE_LIMIT_KEY_PARTS,
        rate_limit::MAX_RATE_LIMIT_KEY_PART_NAME_BYTES,
        rate_limit::MAX_RATE_LIMIT_KEY_BYTES,
        rate_limit::MAX_RATE_LIMIT_WINDOWS_PER_POLICY,
        rate_limit::MAX_RATE_LIMIT_POLICIES_PER_PLAN,
    ];
    let response_limits = [response::MAX_EMPTY_RESPONSE_CHUNKS];
    let retry_limits = [retry::MAX_POLICY_RETRIES_PER_OPERATION as usize];

    assert!(auth_limits.into_iter().all(|limit| limit > 0));
    assert!(body_limits.into_iter().all(|limit| limit > 0));
    #[cfg(feature = "multipart")]
    assert!(multipart_limits.into_iter().all(|limit| limit > 0));
    assert!(endpoint_limits.into_iter().all(|limit| limit > 0));
    assert!(http_limits.into_iter().all(|limit| limit > 0));
    assert!(rate_limits.into_iter().all(|limit| limit > 0));
    assert!(response_limits.into_iter().all(|limit| limit > 0));
    assert!(retry_limits.into_iter().all(|limit| limit > 0));
}

#[cfg(feature = "records")]
#[test]
fn records_contracts_are_exposed_by_the_records_domain() {
    assert!(!public_type::<concord::records::RecordError>().is_empty());
    assert!(!public_type::<concord::records::ItemStream<NonCloneRecord>>().is_empty());
    assert!(!public_type::<PublicRecordDecoder>().is_empty());
}

#[cfg(feature = "json")]
#[test]
fn json_is_exposed_by_the_codec_domain_and_curated_top_level() {
    assert_eq!(
        public_type::<concord::codec::Json>(),
        public_type::<concord::Json>()
    );
}

#[cfg(feature = "multipart")]
#[test]
fn multipart_remains_core_owned_but_is_exposed_by_the_body_domain() {
    assert!(!public_type::<concord::body::Multipart>().is_empty());
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

#[cfg(feature = "rate-limit")]
#[test]
fn local_limiter_is_exposed_by_the_rate_limit_domain() {
    assert!(!public_type::<concord::rate_limit::LocalRateLimiter>().is_empty());
}
