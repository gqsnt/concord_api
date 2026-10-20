#![forbid(unsafe_code)]

use std::any::type_name;

use concord::{
    auth, body, codec, endpoint, error, http, inspection, observation, pagination, rate_limit,
    response, retry, runtime,
};

const _: () = assert!(
    endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES <= endpoint::MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES
);
const _: () =
    assert!(rate_limit::DEFAULT_RATE_LIMIT_KEY_BYTES <= rate_limit::MAX_RATE_LIMIT_KEY_BYTES);

fn public_type<T: ?Sized>() -> &'static str {
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
        public_type::<auth::StoreRequest>(),
        public_type::<body::PreparedBody>(),
        public_type::<body::BodyDescription>(),
        public_type::<codec::CodecErrorKind>(),
        public_type::<codec::Text>(),
        public_type::<endpoint::RuntimeEndpoint>(),
        public_type::<endpoint::Request<()>>(),
        public_type::<endpoint::AcceptedBytesRequest>(),
        public_type::<endpoint::ResponseRequest<()>>(),
        public_type::<endpoint::StoreRequest>(),
        public_type::<error::ApiError>(),
        public_type::<error::ClientInputKind>(),
        public_type::<error::StatusBodyState>(),
        public_type::<error::ProviderAcquisitionFacts>(),
        public_type::<error::ProviderAcquisitionOutcome>(),
        public_type::<observation::ProviderAcquisitionFinished>(),
        public_type::<inspection::ContractDescription>(),
        public_type::<observation::AttemptInfo>(),
        public_type::<observation::DebugLevel>(),
        public_type::<http::Method>(),
        public_type::<pagination::OffsetPagination>(),
        public_type::<pagination::Pages<()>>(),
        public_type::<pagination::CollectPrefix<()>>(),
        public_type::<pagination::CollectAll<()>>(),
        public_type::<pagination::TraversalSummary>(),
        public_type::<rate_limit::NoopRateLimiter>(),
        public_type::<response::Empty>(),
        public_type::<response::NotFound<codec::Text>>(),
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

mod root_exports {
    //! Declared at module level with only facade-root imports (§7.2).
    use concord::{Binary, ByteStream, Bytes, Discard, Empty, Text, api};

    api! {
        client RootExportsApi {
            base "http://root-exports.example.test/";
            timeout 7s;

            GET ping() -> () via Empty;
            GET raw() -> Bytes via Binary;
            GET stream() -> ByteStream via Binary;
            GET text() -> String via Text;
            GET discard() -> () via Discard;
        }
    }
}

/// §7.2: the facade root re-exports the convenience names used by every
/// example, each naming the same type as its semantic domain.
#[test]
fn facade_root_exports_the_required_convenience_names() {
    use concord::{
        ApiError, Binary, ByteStream, Bytes, CallOptions, CancellationToken, ClientBuildError,
        ControlError, ControlSummary, Discard, DispatchKnowledge, Empty, ExecutionMeta, IntoValue,
        OperationId, RequestControls, SecretString, StoreOutcome, Text,
    };
    use root_exports::RootExportsApi;

    assert_eq!(public_type::<ApiError>(), public_type::<error::ApiError>());
    assert_eq!(
        public_type::<ClientBuildError>(),
        public_type::<error::ClientBuildError>()
    );
    assert_eq!(
        public_type::<DispatchKnowledge>(),
        public_type::<error::DispatchKnowledge>()
    );
    assert_eq!(
        public_type::<StoreOutcome>(),
        public_type::<auth::StoreOutcome>()
    );
    assert_eq!(
        public_type::<SecretString>(),
        public_type::<auth::SecretString>()
    );
    assert_eq!(
        public_type::<OperationId>(),
        public_type::<runtime::OperationId>()
    );
    assert_eq!(
        public_type::<ExecutionMeta>(),
        public_type::<response::ExecutionMeta>()
    );
    assert_eq!(public_type::<Bytes>(), public_type::<codec::Bytes>());
    assert_eq!(
        public_type::<ByteStream>(),
        public_type::<response::ByteStream>()
    );
    assert_eq!(public_type::<Binary>(), public_type::<codec::Binary>());
    assert_eq!(public_type::<Text>(), public_type::<codec::Text>());
    assert_eq!(public_type::<Empty>(), public_type::<response::Empty>());
    assert_eq!(public_type::<Discard>(), public_type::<response::Discard>());
    #[cfg(feature = "json")]
    assert_eq!(public_type::<concord::Json>(), public_type::<codec::Json>());

    // Controls: plain values with the documented defaults.
    let options = CallOptions::default().cap_sends(2).cap_sends(5);
    assert_eq!(options.send_cap(), Some(2), "repeated caps intersect");
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());
    token.clone().cancel();
    assert!(token.is_cancelled(), "clones share one signal");
    let _: Option<ControlSummary> = None;
    let _: Option<ControlError> = None;

    // The closed ordinary conversion set (VALUE-04).
    fn convert<T, V: IntoValue<T>>(value: V) -> T {
        value.into_value()
    }
    let owned = String::from("owned");
    assert_eq!(convert::<String, _>("borrowed"), "borrowed");
    assert_eq!(convert::<String, _>(&owned), "owned");
    assert_eq!(convert::<u32, _>(7), 7);
    assert_eq!(convert::<Option<u8>, _>(None), None);

    // The unsealed common controls trait is usable as a generic bound.
    fn summarize<R: RequestControls>(request: &R) -> ControlSummary {
        request.control_summary()
    }

    let client = RootExportsApi::new().expect("root exports client");
    let summary = summarize(&client.ping());
    assert_eq!(
        summary.effective_timeout(),
        Some(std::time::Duration::from_secs(7))
    );
    let _ = (
        client.raw(),
        client.stream(),
        client.text(),
        client.discard(),
    );
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
    assert_eq!(
        public_type::<concord::ItemStream<NonCloneRecord>>(),
        public_type::<concord::records::ItemStream<NonCloneRecord>>()
    );
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
    assert!(!public_type::<concord::body::MultipartBody>().is_empty());
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

/// §7.2: `LocalRateLimiter` is reachable as
/// `concord::rate_limit::LocalRateLimiter` when enabled (a default feature).
#[cfg(feature = "rate-limit")]
#[test]
fn local_limiter_is_exposed_by_the_rate_limit_domain() {
    let limiter: std::sync::Arc<dyn rate_limit::RateLimiter> =
        std::sync::Arc::new(concord::rate_limit::LocalRateLimiter::new());
    drop(limiter);
    assert!(!public_type::<concord::rate_limit::LocalRateLimiter>().is_empty());
}
