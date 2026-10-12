//! Root-build preflight regression coverage. No request is dispatched.
#![cfg(feature = "default-tls")]
use concord::auth::{
    AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest, ProviderOrigin,
};
use concord::error::{ClientBuildErrorKind, ClientInputKind};
use concord::{Binary, ClientBuildError, api};
use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

api! {
    client ReviewAggregate {
        base "https://review.invalid/";
        response Binary;
        values { query a: String; query b: String; }
        rate_limit { id "review.aggregate"; key [origin]; allow 1 per 1s; }
        GET get() -> concord::Bytes { path "resource"; }
    }
}

pub struct CountProvider {
    origins: Arc<AtomicUsize>,
    acquisitions: Arc<AtomicUsize>,
}
impl CredentialProvider for CountProvider {
    type Material = AccessToken;
    fn acquire<'a>(
        &'a self,
        _: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { panic!("build must never acquire credential material") })
    }
    fn http_origins(&self) -> &[ProviderOrigin] {
        self.origins.fetch_add(1, Ordering::SeqCst);
        &[]
    }
}
api! {
    client ReviewProvider {
        base "https://review.invalid/";
        response Binary;
        credentials { credential token = custom CountProvider; }
        GET get() -> concord::Bytes { path "resource"; }
    }
}
fn error<T>(result: Result<T, ClientBuildError>) -> ClientBuildError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("invalid selection was accepted"),
    }
}
fn build_duration(
    configure: impl FnOnce(
        concord::runtime::RuntimeConfigBuilder,
    ) -> concord::runtime::RuntimeConfigBuilder,
) -> Result<ReviewProvider, ClientBuildError> {
    ReviewProvider::builder()
        .token(CountProvider {
            origins: Arc::new(AtomicUsize::new(0)),
            acquisitions: Arc::new(AtomicUsize::new(0)),
        })
        .configure_runtime(configure)
        .build()
}

#[test]
fn independent_missing_runtime_and_limiter_diagnostics_are_all_retained() {
    let error = error(
        ReviewAggregate::builder()
            .configure_runtime(|runtime| {
                runtime
                    .provider_request_limit(0)
                    .provider_response_limit(0)
                    .provider_max_sends(0)
                    .provider_max_origins(0)
                    .provider_timeout(Duration::ZERO)
                    .stream_response_limit(0)
                    .stream_lifetime(Duration::ZERO)
                    .stream_idle_timeout(Duration::ZERO)
                    .stream_max_records(0)
                    .stream_frame_limit(0)
                    .request_body_limit(0)
                    .response_body_limit(0)
                    .status_body_limit(0)
                    .pagination_max_pages(0)
                    .pagination_max_items(0)
                    .pagination_total_response_body_limit(0)
                    .pagination_identity_limit(0)
                    .aggregate_max_sends(0)
                    .runtime_endpoint_timeout(Duration::MAX)
                    .auth_failure_cooldown_cap(Duration::MAX)
                    .rate_limit_cooldown_cap(Duration::MAX)
            })
            .build(),
    );
    assert_eq!(error.kind(), ClientBuildErrorKind::MissingInput);
    assert_eq!(error.input_field(), Some("set_a"));
    assert_eq!(error.input_kind(), Some(ClientInputKind::Value));
    assert_eq!(
        error.missing_fields(),
        vec![
            ("set_a", ClientInputKind::Value),
            ("set_b", ClientInputKind::Value)
        ]
    );
    let diagnostics: Vec<_> = error.diagnostics().collect();
    assert_eq!(diagnostics.len(), 24, "{error:?}");
    assert_eq!(
        diagnostics
            .iter()
            .filter(|d| d.kind() == ClientBuildErrorKind::LimiterUnavailable)
            .count(),
        1
    );
    let mut runtime_names: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.kind() == ClientBuildErrorKind::RuntimeConfiguration)
        .map(|d| d.input_field().expect("static setter name"))
        .collect();
    runtime_names.sort_unstable();
    let mut expected = vec![
        "provider_request_limit",
        "provider_response_limit",
        "provider_max_sends",
        "provider_max_origins",
        "provider_timeout",
        "stream_response_limit",
        "stream_lifetime",
        "stream_idle_timeout",
        "stream_max_records",
        "stream_frame_limit",
        "request_body_limit",
        "response_body_limit",
        "status_body_limit",
        "pagination_max_pages",
        "pagination_max_items",
        "pagination_total_response_body_limit",
        "pagination_identity_limit",
        "aggregate_max_sends",
        "runtime_endpoint_timeout",
        "auth_failure_cooldown_cap",
        "rate_limit_cooldown_cap",
    ];
    expected.sort_unstable();
    assert_eq!(runtime_names, expected);
}

#[test]
fn invalid_config_stops_before_provider_callbacks_and_valid_build_is_positive_control() {
    let origins = Arc::new(AtomicUsize::new(0));
    let acquisitions = Arc::new(AtomicUsize::new(0));
    let provider = || CountProvider {
        origins: origins.clone(),
        acquisitions: acquisitions.clone(),
    };
    let failure = error(
        ReviewProvider::builder()
            .token(provider())
            .configure_runtime(|runtime| {
                runtime
                    .request_body_limit(0)
                    .stream_lifetime(Duration::ZERO)
            })
            .build(),
    );
    assert_eq!(failure.diagnostics().count(), 2);
    assert_eq!(origins.load(Ordering::SeqCst), 0);
    assert_eq!(acquisitions.load(Ordering::SeqCst), 0);
    ReviewProvider::builder()
        .token(provider())
        .build()
        .expect("valid provider build");
    assert_eq!(origins.load(Ordering::SeqCst), 1);
    assert_eq!(acquisitions.load(Ordering::SeqCst), 0);
}

#[test]
fn trusted_duration_replacements_validate_latest_and_zero_cooldowns_are_valid() {
    for duration in [Duration::ZERO, Duration::MAX] {
        let failure = error(build_duration(move |runtime| {
            runtime
                .runtime_endpoint_timeout(Duration::from_secs(1))
                .runtime_endpoint_timeout(duration)
        }));
        assert_eq!(failure.kind(), ClientBuildErrorKind::RuntimeConfiguration);
        assert_eq!(failure.input_field(), Some("runtime_endpoint_timeout"));
        build_duration(move |runtime| {
            runtime
                .runtime_endpoint_timeout(duration)
                .runtime_endpoint_timeout(Duration::from_secs(1))
        })
        .expect("valid later timeout replaces invalid selection");
    }
    build_duration(|runtime| {
        runtime
            .auth_failure_cooldown_cap(Duration::MAX)
            .auth_failure_cooldown_cap(Duration::ZERO)
            .rate_limit_cooldown_cap(Duration::MAX)
            .rate_limit_cooldown_cap(Duration::ZERO)
    })
    .expect("explicit zero disables cooldown retention");
}

#[test]
fn native_http_duration_errors_are_independent_and_latest_valid_zero_replaces_max() {
    let provider = || CountProvider {
        origins: Arc::new(AtomicUsize::new(0)),
        acquisitions: Arc::new(AtomicUsize::new(0)),
    };
    let failure = error(
        ReviewProvider::builder()
            .token(provider())
            .configure_http(|http| {
                http.connect_timeout(Duration::MAX)
                    .read_timeout(Duration::MAX)
                    .pool_idle_timeout(Duration::MAX)
                    .tcp_keepalive(Duration::MAX)
            })
            .build(),
    );
    assert_eq!(failure.kind(), ClientBuildErrorKind::HttpConfiguration);
    assert_eq!(failure.input_field(), Some("connect_timeout"));
    assert_eq!(
        failure
            .diagnostics()
            .map(|d| (d.kind(), d.input_field()))
            .collect::<Vec<_>>(),
        vec![
            (
                ClientBuildErrorKind::HttpConfiguration,
                Some("connect_timeout")
            ),
            (
                ClientBuildErrorKind::HttpConfiguration,
                Some("read_timeout")
            ),
            (
                ClientBuildErrorKind::HttpConfiguration,
                Some("pool_idle_timeout")
            ),
            (
                ClientBuildErrorKind::HttpConfiguration,
                Some("tcp_keepalive")
            ),
        ]
    );
    ReviewProvider::builder()
        .token(provider())
        .configure_http(|http| {
            http.connect_timeout(Duration::MAX)
                .connect_timeout(Duration::ZERO)
                .read_timeout(Duration::MAX)
                .read_timeout(Duration::ZERO)
                .pool_idle_timeout(Duration::MAX)
                .pool_idle_timeout(Duration::ZERO)
                .tcp_keepalive(Duration::MAX)
                .tcp_keepalive(Duration::ZERO)
        })
        .build()
        .expect("valid zero latest native controls preserve prior semantics");
}

#[test]
fn both_malformed_tls_material_diagnostics_are_safe_and_precede_provider_callbacks() {
    let origins = Arc::new(AtomicUsize::new(0));
    let failure = error(
        ReviewProvider::builder()
            .token(CountProvider {
                origins: origins.clone(),
                acquisitions: Arc::new(AtomicUsize::new(0)),
            })
            .configure_http(|http| {
                http.add_root_certificate_pem(b"root-private-sentinel")
                    .identity_pem(b"identity-private-sentinel")
            })
            .build(),
    );
    assert_eq!(
        failure.diagnostics().map(|d| d.kind()).collect::<Vec<_>>(),
        vec![
            ClientBuildErrorKind::InvalidRootCertificate,
            ClientBuildErrorKind::InvalidClientIdentity
        ]
    );
    assert_eq!(origins.load(Ordering::SeqCst), 0);
    for rendered in [failure.to_string(), format!("{failure:?}")] {
        assert!(!rendered.contains("private-sentinel"));
    }
    assert!(std::error::Error::source(&failure).is_none());
}
