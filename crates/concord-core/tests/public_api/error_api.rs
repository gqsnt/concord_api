use std::error::Error;
use std::time::Duration;

use concord_core::contracts::{
    ApiError, AuthErrorKind, Bytes, ClientBuildError, ClientBuildErrorKind, ClientInputKind,
    CodecError, CodecErrorKind, ConfigurationError, ConfigurationErrorKind,
    CredentialMaterialError, CredentialMaterialErrorKind, DecodeBody, DecodeContext, EndpointId,
    ErrorCategory, HeaderName, HttpResponseFailure, Method, PaginationErrorKind,
    RateLimitErrorKind, SanitizedHeaderMap, StatusBody, StatusBodyDecodeError, StatusBodyState,
};

fn api_error_accessors(error: &ApiError) {
    let _: ErrorCategory = error.category();
    let _: Option<ConfigurationErrorKind> = error.configuration_kind();
    let _: &str = error.endpoint();
    let _: &EndpointId = error.endpoint_id();
    let _: &Method = error.method();
    let _: Option<concord_core::contracts::StatusCode> = error.http_status();
    let _: Option<&SanitizedHeaderMap> = error.http_headers();
    let _: Option<&HttpResponseFailure> = error.http_response();
    let _: Option<AuthErrorKind> = error.auth_kind();
    let _: Option<RateLimitErrorKind> = error.rate_limit_kind();
    let _: Option<PaginationErrorKind> = error.pagination_kind();
    let _: Option<Duration> = error.retry_after();
}

fn response_accessors(response: &HttpResponseFailure) {
    let _: &str = response.endpoint();
    let _: &EndpointId = response.endpoint_id();
    let _: &Method = response.method();
    let _: concord_core::contracts::StatusCode = response.status();
    let _: &SanitizedHeaderMap = response.headers();
    let _: &StatusBody = response.body();
    let _: Option<Duration> = response.retry_after();
}

fn client_build_accessors(error: &ClientBuildError) {
    let _: ClientBuildErrorKind = error.kind();
    let _: Option<&'static str> = error.input_field();
    let _: Option<ClientInputKind> = error.input_kind();
}

fn owning_accessors(error: ApiError, response: HttpResponseFailure, body: StatusBody) {
    let _: Result<HttpResponseFailure, ApiError> = error.into_http_response();
    let _: StatusBody = response.into_body();
    let _: Option<Bytes> = body.into_bytes();
}

fn status_body_accessors(body: &StatusBody) {
    let _: StatusBodyState = body.state();
    let _: Option<usize> = body.len();
    let _: Option<&Bytes> = body.bytes();
}

struct DownstreamDecoder;

impl DecodeBody for DownstreamDecoder {
    type Output = usize;

    const ACCEPT: Option<&'static str> = Some("application/downstream");

    fn decode(body: Bytes, context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let _: &str = context.endpoint;
        let _: &Method = context.method;
        let _ = context.status;
        let _: Option<&str> = context.content_type;
        Ok(body.len())
    }
}

fn decode_accessor(response: &HttpResponseFailure) {
    let _: Result<usize, StatusBodyDecodeError> = response.decode_body::<DownstreamDecoder>();
}

fn assert_error_traits<T: Error + Send + Sync + 'static>() {}
fn assert_send_sync_static<T: Send + Sync + 'static>() {}
fn assert_value_traits<T: Clone + Copy + std::fmt::Debug + Eq + PartialEq>() {}

#[test]
fn downstream_surface_has_exact_structured_accessors_and_traits() {
    let _: fn(&ApiError) = api_error_accessors;
    let _: fn(&HttpResponseFailure) = response_accessors;
    let _: fn(&ClientBuildError) = client_build_accessors;
    let _: fn(ApiError, HttpResponseFailure, StatusBody) = owning_accessors;
    let _: fn(&StatusBody) = status_body_accessors;
    let _: fn(&HttpResponseFailure) = decode_accessor;

    assert_error_traits::<ApiError>();
    assert_error_traits::<ClientBuildError>();
    assert_error_traits::<CredentialMaterialError>();
    assert_error_traits::<ConfigurationError>();
    assert_error_traits::<CodecError>();
    assert_error_traits::<StatusBodyDecodeError>();
    assert_send_sync_static::<HttpResponseFailure>();
    assert_send_sync_static::<StatusBody>();
    assert_send_sync_static::<SanitizedHeaderMap>();
    assert_value_traits::<ErrorCategory>();
    assert_value_traits::<AuthErrorKind>();
    assert_value_traits::<RateLimitErrorKind>();
    assert_value_traits::<PaginationErrorKind>();
    assert_value_traits::<ClientBuildErrorKind>();
    assert_value_traits::<ClientInputKind>();
    assert_value_traits::<CredentialMaterialErrorKind>();
    assert_value_traits::<ConfigurationErrorKind>();
    assert_value_traits::<CodecErrorKind>();
    assert_value_traits::<StatusBodyState>();

    let codec_error = CodecError::new(CodecErrorKind::Limit);
    assert_eq!(codec_error.kind(), CodecErrorKind::Limit);

    let _: Option<HeaderName> = None;
}

#[test]
fn public_categories_are_matched_structurally() {
    let category = ErrorCategory::HttpStatus;
    let label = match category {
        ErrorCategory::HttpStatus => "status",
        ErrorCategory::Transport => "transport",
        _ => "other",
    };
    assert_eq!(label, "status");
}

#[test]
fn pagination_ceiling_kinds_are_public_and_structured() {
    let kinds = [
        PaginationErrorKind::Controller,
        PaginationErrorKind::InvalidInitialCursor,
        PaginationErrorKind::IneligibleEndpoint,
        PaginationErrorKind::ForbiddenContinuationChange,
        PaginationErrorKind::SendCapExhausted,
        PaginationErrorKind::Cancelled,
        PaginationErrorKind::Overflow,
        PaginationErrorKind::RepeatedRequest,
        PaginationErrorKind::CompletionLimitExceeded,
        PaginationErrorKind::AttemptCeilingExceeded,
        PaginationErrorKind::CollectedItemCeilingExceeded,
        PaginationErrorKind::IdentityStorageCeilingExceeded,
    ];

    for (index, kind) in kinds.iter().enumerate() {
        assert!(
            kinds[index + 1..].iter().all(|candidate| candidate != kind),
            "named pagination classifications must remain structurally distinct"
        );
    }
    assert_ne!(
        PaginationErrorKind::AttemptCeilingExceeded,
        PaginationErrorKind::CompletionLimitExceeded
    );
    assert_ne!(
        PaginationErrorKind::CollectedItemCeilingExceeded,
        PaginationErrorKind::IdentityStorageCeilingExceeded
    );
}

#[test]
fn client_build_kinds_are_public_exact_and_structurally_distinct() {
    let kinds = [
        ClientBuildErrorKind::MissingInput,
        ClientBuildErrorKind::InvalidCredential,
        ClientBuildErrorKind::InvalidBaseUrl,
        ClientBuildErrorKind::HttpsRequired,
        ClientBuildErrorKind::TlsUnavailable,
        ClientBuildErrorKind::InvalidProxy,
        ClientBuildErrorKind::InvalidRootCertificate,
        ClientBuildErrorKind::InvalidClientIdentity,
        ClientBuildErrorKind::HttpClientBuild,
        ClientBuildErrorKind::RuntimeConfiguration,
        ClientBuildErrorKind::LimiterUnavailable,
        ClientBuildErrorKind::Internal,
    ];

    for (index, kind) in kinds.iter().enumerate() {
        assert!(
            kinds[index + 1..].iter().all(|candidate| candidate != kind),
            "named client-build classifications must remain structurally distinct"
        );
    }
}
