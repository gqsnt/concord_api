use crate::support::assert_error_chain_does_not_contain_any;
use concord_core::auth::{AuthError, AuthErrorKind};
use concord_core::error::{ApiClientError, ErrorCategory, ErrorContext, PaginationErrorKind};
use concord_core::prelude::RateLimitErrorKind;
use concord_core::transport::{TransportError, TransportErrorKind};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use std::error::Error;
use std::fmt::{self, Debug, Display};

const CODEC_SENTINEL: &str = "ERR_TAXONOMY_CODEC_SENTINEL";
const DECODE_SENTINEL: &str = "ERR_TAXONOMY_DECODE_SENTINEL";
const TRANSPORT_SENTINEL: &str = "ERR_TAXONOMY_TRANSPORT_SENTINEL";
const RATE_LIMIT_SENTINEL: &str = "ERR_TAXONOMY_RATE_LIMIT_SENTINEL";

#[derive(Clone, Copy)]
struct HiddenSourceError {
    label: &'static str,
    sentinel: &'static str,
}

impl HiddenSourceError {
    const fn new(label: &'static str, sentinel: &'static str) -> Self {
        Self { label, sentinel }
    }

    const fn label(&self) -> &'static str {
        self.label
    }
}

impl Display for HiddenSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.sentinel;
        f.write_str(self.label)
    }
}

impl Debug for HiddenSourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self.sentinel;
        f.debug_struct("HiddenSourceError")
            .field("label", &self.label)
            .finish()
    }
}

impl Error for HiddenSourceError {}

fn ctx() -> ErrorContext {
    ErrorContext {
        endpoint: "Taxonomy",
        method: Method::GET,
    }
}

fn assert_surface_markers(rendered: &str, surface: &str, markers: &[&str]) {
    for (idx, marker) in markers.iter().enumerate() {
        assert!(
            rendered.contains(marker),
            "{surface} missing expected marker at index {idx}"
        );
    }
}

fn assert_public_rendering(err: &ApiClientError, display_markers: &[&str], debug_markers: &[&str]) {
    let display = err.to_string();
    let debug = format!("{err:?}");
    assert_surface_markers(&display, "Display", display_markers);
    assert_surface_markers(&debug, "Debug", debug_markers);
}

#[test]
fn public_error_taxonomy_and_accessors_cover_major_variants() {
    let codec_err = ApiClientError::codec_error(
        ctx(),
        HiddenSourceError::new("codec source", CODEC_SENTINEL),
    );
    assert!(matches!(codec_err, ApiClientError::Codec { .. }));
    assert_eq!(codec_err.category(), ErrorCategory::Decode);
    assert_eq!(codec_err.context().endpoint, "Taxonomy");
    assert_eq!(codec_err.context().method, Method::GET);
    assert_public_rendering(
        &codec_err,
        &["GET Taxonomy", "codec"],
        &["Codec", "Taxonomy"],
    );
    let codec_source = codec_err.source().expect("codec source should exist");
    let codec_source = codec_source
        .downcast_ref::<HiddenSourceError>()
        .expect("codec source should be typed");
    assert_eq!(codec_source.label(), "codec source");

    let decode_err = ApiClientError::decode_error(
        ctx(),
        StatusCode::BAD_GATEWAY,
        Some("application/json"),
        HiddenSourceError::new("decode source", DECODE_SENTINEL),
    );
    assert!(matches!(decode_err, ApiClientError::Decode { .. }));
    assert_eq!(decode_err.category(), ErrorCategory::Decode);
    assert_eq!(decode_err.decode_status(), Some(StatusCode::BAD_GATEWAY));
    assert_eq!(decode_err.decode_content_type(), Some("application/json"));
    assert_public_rendering(
        &decode_err,
        &[
            "GET Taxonomy",
            "decode error",
            "status=502",
            "content-type=application/json",
        ],
        &["Decode", "Taxonomy"],
    );
    let contextual = decode_err.source().expect("decode source should exist");
    let decode_source = contextual
        .source()
        .expect("nested decode source should exist");
    let decode_source = decode_source
        .downcast_ref::<HiddenSourceError>()
        .expect("decode source should be typed");
    assert_eq!(decode_source.label(), "decode source");

    let mut headers = HeaderMap::new();
    headers.insert("x-public", HeaderValue::from_static("public-value"));
    let expected_header = HeaderValue::from_static("public-value");
    let http_status_err = ApiClientError::HttpStatus {
        ctx: ctx(),
        status: StatusCode::SERVICE_UNAVAILABLE,
        headers: Box::new(headers),
        rate_limit: None,
    };
    assert!(matches!(http_status_err, ApiClientError::HttpStatus { .. }));
    assert_eq!(http_status_err.category(), ErrorCategory::HttpStatus);
    assert_eq!(
        http_status_err.http_status(),
        Some(StatusCode::SERVICE_UNAVAILABLE)
    );
    assert_eq!(
        http_status_err
            .http_headers()
            .and_then(|headers| headers.get("x-public")),
        Some(&expected_header)
    );
    assert_public_rendering(
        &http_status_err,
        &["GET Taxonomy", "status 503"],
        &["HttpStatus", "Taxonomy"],
    );

    let transport_err = ApiClientError::Transport {
        ctx: ctx(),
        source: TransportError::with_kind(
            TransportErrorKind::Timeout,
            HiddenSourceError::new("transport source", TRANSPORT_SENTINEL),
        ),
    };
    assert!(matches!(transport_err, ApiClientError::Transport { .. }));
    assert_eq!(transport_err.category(), ErrorCategory::Timeout);
    assert_public_rendering(
        &transport_err,
        &["GET Taxonomy", "transport"],
        &["Transport", "Timeout"],
    );
    match &transport_err {
        ApiClientError::Transport { source, .. } => {
            assert_eq!(source.kind(), TransportErrorKind::Timeout);
            let source = source.source_error();
            let source = source
                .downcast_ref::<HiddenSourceError>()
                .expect("transport source should be typed");
            assert_eq!(source.label(), "transport source");
        }
        _ => unreachable!(),
    }

    let missing_credential_err = ApiClientError::Auth {
        ctx: ctx(),
        source: AuthError::new(
            AuthErrorKind::MissingCredential,
            "missing credential for configured auth",
        ),
    };
    assert!(matches!(
        missing_credential_err,
        ApiClientError::Auth { .. }
    ));
    assert_eq!(
        missing_credential_err.category(),
        ErrorCategory::MissingCredential
    );
    assert_public_rendering(
        &missing_credential_err,
        &["GET Taxonomy", "auth"],
        &["Auth", "Taxonomy"],
    );
    match &missing_credential_err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::MissingCredential);
        }
        _ => unreachable!(),
    }

    let rejected_err = ApiClientError::Auth {
        ctx: ctx(),
        source: AuthError::new(AuthErrorKind::RejectedCredential, "auth challenge rejected"),
    };
    assert!(matches!(rejected_err, ApiClientError::Auth { .. }));
    assert_eq!(rejected_err.category(), ErrorCategory::AuthRejected);
    assert_public_rendering(
        &rejected_err,
        &["GET Taxonomy", "auth"],
        &["Auth", "Taxonomy"],
    );
    match &rejected_err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::RejectedCredential);
        }
        _ => unreachable!(),
    }

    let acquire_failed_err = ApiClientError::Auth {
        ctx: ctx(),
        source: AuthError::new(AuthErrorKind::AcquireFailed, "auth acquire failed"),
    };
    assert!(matches!(acquire_failed_err, ApiClientError::Auth { .. }));
    assert_eq!(acquire_failed_err.category(), ErrorCategory::AuthRejected);
    assert_public_rendering(
        &acquire_failed_err,
        &["GET Taxonomy", "auth"],
        &["Auth", "Taxonomy"],
    );
    match &acquire_failed_err {
        ApiClientError::Auth { source, .. } => {
            assert_eq!(source.kind, AuthErrorKind::AcquireFailed);
        }
        _ => unreachable!(),
    }

    let rate_limit_err = ApiClientError::rate_limit_with_source(
        ctx(),
        RateLimitErrorKind::AcquireFailed,
        "rate limit acquire failed",
        HiddenSourceError::new("rate limit source", RATE_LIMIT_SENTINEL),
    );
    assert!(matches!(rate_limit_err, ApiClientError::RateLimit { .. }));
    assert_eq!(rate_limit_err.category(), ErrorCategory::RateLimit);
    assert_eq!(
        rate_limit_err
            .rate_limit_error()
            .map(|source| source.kind()),
        Some(RateLimitErrorKind::AcquireFailed)
    );
    assert_public_rendering(
        &rate_limit_err,
        &["GET Taxonomy", "rate limit"],
        &["RateLimit", "Taxonomy"],
    );
    let rate_limit_source = rate_limit_err
        .rate_limit_error()
        .expect("rate-limit source should exist");
    let rate_limit_source = rate_limit_source
        .source()
        .expect("rate-limit nested source should exist");
    let rate_limit_source = rate_limit_source
        .downcast_ref::<HiddenSourceError>()
        .expect("rate-limit source should be typed");
    assert_eq!(rate_limit_source.label(), "rate limit source");

    let no_content_err = ApiClientError::NoContentStatusRequiresNoContent {
        ctx: ctx(),
        status: StatusCode::NO_CONTENT,
    };
    assert!(matches!(
        no_content_err,
        ApiClientError::NoContentStatusRequiresNoContent { .. }
    ));
    assert_eq!(no_content_err.category(), ErrorCategory::ResponseContract);
    assert_public_rendering(
        &no_content_err,
        &["GET Taxonomy", "no content"],
        &["NoContentStatusRequiresNoContent", "Taxonomy"],
    );

    let pagination_err = ApiClientError::pagination(
        ctx(),
        PaginationErrorKind::NonProgress,
        "pagination did not make progress",
    );
    assert!(matches!(pagination_err, ApiClientError::Pagination { .. }));
    assert_eq!(pagination_err.category(), ErrorCategory::Pagination);
    assert_eq!(
        pagination_err.pagination_error_kind(),
        Some(PaginationErrorKind::NonProgress)
    );
    assert_public_rendering(
        &pagination_err,
        &["GET Taxonomy", "pagination"],
        &["Pagination", "Taxonomy"],
    );
}

#[test]
fn public_error_source_chains_are_accessible_and_redacted() {
    let codec_err = ApiClientError::codec_error(
        ctx(),
        HiddenSourceError::new("codec source", CODEC_SENTINEL),
    );
    assert_error_chain_does_not_contain_any(&codec_err, &[CODEC_SENTINEL]);

    let decode_err = ApiClientError::decode_error(
        ctx(),
        StatusCode::BAD_GATEWAY,
        Some("application/json"),
        HiddenSourceError::new("decode source", DECODE_SENTINEL),
    );
    assert_error_chain_does_not_contain_any(&decode_err, &[DECODE_SENTINEL]);

    let transport_err = ApiClientError::Transport {
        ctx: ctx(),
        source: TransportError::with_kind(
            TransportErrorKind::Timeout,
            HiddenSourceError::new("transport source", TRANSPORT_SENTINEL),
        ),
    };
    assert_error_chain_does_not_contain_any(&transport_err, &[TRANSPORT_SENTINEL]);

    let rate_limit_err = ApiClientError::rate_limit_with_source(
        ctx(),
        RateLimitErrorKind::AcquireFailed,
        "rate limit acquire failed",
        HiddenSourceError::new("rate limit source", RATE_LIMIT_SENTINEL),
    );
    assert_error_chain_does_not_contain_any(&rate_limit_err, &[RATE_LIMIT_SENTINEL]);
}
