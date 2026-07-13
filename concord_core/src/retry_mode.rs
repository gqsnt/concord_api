//! Client-level general retry authority.
//!
//! Concord no longer owns a general HTTP retry loop. General retries are a
//! property of the managed Reqwest client, selected once at construction time
//! through [`RetryMode`]. Exactly three modes are supported:
//!
//! * [`RetryMode::ProtocolRecovery`] — the default. Concord installs no custom
//!   Reqwest retry policy, preserving Reqwest 0.13.4's built-in safe protocol
//!   recovery. Concord does not promise a stable retry budget or physical-send
//!   count for this mode.
//! * [`RetryMode::Disabled`] — installs [`reqwest::retry::never`]. This is the
//!   mode for exact visible-to-physical send accounting.
//! * [`RetryMode::Status`] — installs one scoped custom Reqwest policy that
//!   retries a constrained set of gateway statuses for safe methods only.
//!
//! The bounded Concord-owned authentication recovery is independent of every
//! mode: it is always a visible second call to `reqwest::Client::execute`.

use crate::transport::ReqwestClientBuildError;
use http::{Method, StatusCode};
use std::error::Error;
use std::fmt;

/// A protocol scheme known without consulting runtime client values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginScheme {
    Http,
    Https,
}

/// Safe static origin metadata for a fixed single-origin client context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedOriginDescriptor {
    pub scheme: OriginScheme,
    pub authority: &'static str,
}

/// Static origin classification used to validate client-level status retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiOriginDescriptor {
    FixedSingleOrigin(FixedOriginDescriptor),
    DynamicOrigin,
    MultiOrigin,
}

/// Approved statuses for [`RetryMode::Status`]. Selecting `503` means an
/// immediate Reqwest retry; the hidden retry does not inspect or honor the
/// response's `Retry-After` header.
const APPROVED_STATUSES: [StatusCode; 3] = [
    StatusCode::BAD_GATEWAY,
    StatusCode::SERVICE_UNAVAILABLE,
    StatusCode::GATEWAY_TIMEOUT,
];

/// The general retry policy installed on a managed Reqwest client.
///
/// Retry policy is a client-construction decision, not an endpoint execution
/// property. [`ProtocolRecovery`](RetryMode::ProtocolRecovery) is the default.
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum RetryMode {
    /// Preserve Reqwest's built-in safe protocol recovery. No custom policy is
    /// installed. This mode does not have a stable, promised physical-send
    /// count.
    #[default]
    ProtocolRecovery,
    /// Install [`reqwest::retry::never`]; every visible execution produces
    /// exactly one physical send (before authentication recovery).
    Disabled,
    /// Install one scoped custom Reqwest policy for the generated client's
    /// fixed host.
    Status(StatusRetryConfig),
}

impl RetryMode {
    /// Convenience constructor for [`RetryMode::Status`].
    pub fn status(
        max_retries: u8,
        statuses: impl IntoIterator<Item = StatusCode>,
    ) -> Result<Self, RetryModeError> {
        Ok(Self::Status(StatusRetryConfig::new(max_retries, statuses)?))
    }

    /// Resolves this mode against an API's static origin classification,
    /// producing the concrete Reqwest install. [`RetryMode::Status`] is only
    /// permitted for a fixed single-origin API.
    pub(crate) fn resolve(
        &self,
        origin: ApiOriginDescriptor,
    ) -> Result<ReqwestRetryInstall, RetryModeError> {
        match self {
            RetryMode::ProtocolRecovery => Ok(ReqwestRetryInstall::ProtocolRecovery),
            RetryMode::Disabled => Ok(ReqwestRetryInstall::Never),
            RetryMode::Status(config) => {
                let authority = match origin {
                    ApiOriginDescriptor::FixedSingleOrigin(fixed) => fixed.authority,
                    ApiOriginDescriptor::DynamicOrigin | ApiOriginDescriptor::MultiOrigin => {
                        return Err(RetryModeError::NotFixedOrigin);
                    }
                };
                let host = fixed_origin_host(authority)?;
                Ok(ReqwestRetryInstall::Custom(
                    config.build_reqwest_policy(host),
                ))
            }
        }
    }
}

fn fixed_origin_host(authority: &'static str) -> Result<String, RetryModeError> {
    // Generated descriptors have already passed the semantic URL validator.
    // Repeat the narrow structural check here because hand-written
    // `ClientContext` implementations can provide equivalent metadata.
    if authority.is_empty()
        || authority.contains('@')
        || authority
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(RetryModeError::InvalidFixedOriginMetadata);
    }
    let url = url::Url::parse(&format!("http://{authority}/"))
        .map_err(|_| RetryModeError::InvalidFixedOriginMetadata)?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.host().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(RetryModeError::InvalidFixedOriginMetadata);
    }
    let authority = authority
        .parse::<http::uri::Authority>()
        .map_err(|_| RetryModeError::InvalidFixedOriginMetadata)?;
    let host = authority.host();
    if host.is_empty() {
        return Err(RetryModeError::InvalidFixedOriginMetadata);
    }
    Ok(host.to_owned())
}

/// Constrained controls for [`RetryMode::Status`].
///
/// Only the approved controls are exposed. Arbitrary classifiers, retry
/// budgets, budget percentages, transport-error lists, method lists,
/// idempotency modes, and `Retry-After` retry switches are intentionally
/// absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusRetryConfig {
    max_retries: u8,
    statuses: Vec<StatusCode>,
}

impl StatusRetryConfig {
    /// Builds a validated status-retry configuration.
    ///
    /// Validation:
    /// * `max_retries` must be in `1..=2`;
    /// * the status set must be non-empty;
    /// * every status must be one of `502`, `503`, or `504`.
    pub fn new(
        max_retries: u8,
        statuses: impl IntoIterator<Item = StatusCode>,
    ) -> Result<Self, RetryModeError> {
        if !(1..=2).contains(&max_retries) {
            return Err(RetryModeError::InvalidMaxRetries(max_retries));
        }
        let mut collected = Vec::new();
        for status in statuses {
            if !APPROVED_STATUSES.contains(&status) {
                return Err(RetryModeError::UnsupportedStatus(status.as_u16()));
            }
            if !collected.contains(&status) {
                collected.push(status);
            }
        }
        if collected.is_empty() {
            return Err(RetryModeError::EmptyStatusSet);
        }
        Ok(Self {
            max_retries,
            statuses: collected,
        })
    }

    /// The approved per-request maximum (1 or 2).
    #[inline]
    pub fn max_retries(&self) -> u8 {
        self.max_retries
    }

    /// The approved status set.
    #[inline]
    pub fn statuses(&self) -> &[StatusCode] {
        &self.statuses
    }

    /// Status retry eligibility is internally restricted to safe methods.
    #[inline]
    fn method_is_eligible(method: &Method) -> bool {
        *method == Method::GET || *method == Method::HEAD || *method == Method::OPTIONS
    }

    /// Builds the single scoped Reqwest custom policy for this configuration.
    ///
    /// The policy is scoped to the fixed host, classifies only approved
    /// method/status combinations, uses Reqwest's standard custom-policy
    /// budget, and sets the approved per-request maximum. It replaces, rather
    /// than supplements, Reqwest's default protocol recovery.
    fn build_reqwest_policy(&self, host: String) -> reqwest::retry::Builder {
        // Reqwest scopes by host only (no port); redirects are disabled so a
        // fixed-origin client cannot silently move a status retry elsewhere.
        let statuses = self.statuses.clone();
        reqwest::retry::for_host(host)
            .max_retries_per_request(u32::from(self.max_retries))
            .classify_fn(move |req_rep| {
                let method_ok = Self::method_is_eligible(req_rep.method());
                match req_rep.status() {
                    Some(status) if method_ok && statuses.contains(&status) => req_rep.retryable(),
                    _ => req_rep.success(),
                }
            })
    }
}

/// The resolved managed-client retry install. It is consumed once during
/// managed-client construction.
pub(crate) enum ReqwestRetryInstall {
    /// Install no custom policy; keep Reqwest's built-in protocol recovery.
    ProtocolRecovery,
    /// Install [`reqwest::retry::never`].
    Never,
    /// Install one scoped custom policy.
    Custom(reqwest::retry::Builder),
}

/// A retry-mode configuration or eligibility failure.
#[derive(Debug)]
pub enum RetryModeError {
    /// `max_retries` was not in `1..=2`.
    InvalidMaxRetries(u8),
    /// The configured status set was empty.
    EmptyStatusSet,
    /// A configured status was not one of `502`, `503`, or `504`.
    UnsupportedStatus(u16),
    /// [`RetryMode::Status`] was selected for an API that is not classified as
    /// fixed single-origin by the generated integration contract.
    NotFixedOrigin,
    /// A hand-written fixed-origin descriptor did not contain a structurally
    /// valid HTTP authority.
    InvalidFixedOriginMetadata,
    /// The managed Reqwest client could not be constructed.
    Build(ReqwestClientBuildError),
}

impl From<ReqwestClientBuildError> for RetryModeError {
    fn from(value: ReqwestClientBuildError) -> Self {
        Self::Build(value)
    }
}

impl fmt::Display for RetryModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidMaxRetries(value) => {
                write!(
                    f,
                    "status retry max_retries must be between 1 and 2, got {value}"
                )
            }
            Self::EmptyStatusSet => f.write_str("status retry requires a non-empty status set"),
            Self::UnsupportedStatus(status) => write!(
                f,
                "status retry only supports 502, 503, and 504; got {status}"
            ),
            Self::NotFixedOrigin => {
                f.write_str("status retry mode is only permitted for a fixed single-origin API")
            }
            Self::InvalidFixedOriginMetadata => {
                f.write_str("status retry mode requires valid verified fixed-origin metadata")
            }
            Self::Build(_) => f.write_str("managed reqwest client construction failed"),
        }
    }
}

impl Error for RetryModeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Build(source) => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::{FixedOriginDescriptor, OriginScheme};

    fn fixed(authority: &'static str) -> ApiOriginDescriptor {
        ApiOriginDescriptor::FixedSingleOrigin(FixedOriginDescriptor {
            scheme: OriginScheme::Https,
            authority,
        })
    }

    #[test]
    fn default_is_protocol_recovery() {
        assert_eq!(RetryMode::default(), RetryMode::ProtocolRecovery);
    }

    #[test]
    fn protocol_recovery_installs_no_custom_policy() {
        let install = RetryMode::ProtocolRecovery
            .resolve(ApiOriginDescriptor::DynamicOrigin)
            .expect("protocol recovery always resolves");
        assert!(matches!(install, ReqwestRetryInstall::ProtocolRecovery));
    }

    #[test]
    fn disabled_installs_never() {
        let install = RetryMode::Disabled
            .resolve(ApiOriginDescriptor::MultiOrigin)
            .expect("disabled always resolves");
        assert!(matches!(install, ReqwestRetryInstall::Never));
    }

    #[test]
    fn status_validates_max_retries() {
        for max in [1, 2] {
            assert!(StatusRetryConfig::new(max, [StatusCode::BAD_GATEWAY]).is_ok());
        }
        for max in [0, 3, u8::MAX] {
            assert!(matches!(
                StatusRetryConfig::new(max, [StatusCode::BAD_GATEWAY]),
                Err(RetryModeError::InvalidMaxRetries(_))
            ));
        }
    }

    #[test]
    fn status_rejects_empty_and_unsupported_status_sets() {
        assert!(matches!(
            StatusRetryConfig::new(1, []),
            Err(RetryModeError::EmptyStatusSet)
        ));
        for bad in [
            StatusCode::OK,
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            assert!(matches!(
                StatusRetryConfig::new(1, [bad]),
                Err(RetryModeError::UnsupportedStatus(_))
            ));
        }
    }

    #[test]
    fn status_accepts_only_gateway_statuses() {
        let config = StatusRetryConfig::new(
            2,
            [
                StatusCode::BAD_GATEWAY,
                StatusCode::SERVICE_UNAVAILABLE,
                StatusCode::GATEWAY_TIMEOUT,
            ],
        )
        .expect("gateway statuses are approved");
        assert_eq!(config.statuses().len(), 3);
        assert_eq!(config.max_retries(), 2);
    }

    #[test]
    fn status_requires_fixed_single_origin() {
        let mode = RetryMode::status(2, [StatusCode::SERVICE_UNAVAILABLE]).unwrap();
        assert!(matches!(
            mode.resolve(ApiOriginDescriptor::DynamicOrigin),
            Err(RetryModeError::NotFixedOrigin)
        ));
        assert!(matches!(
            mode.resolve(ApiOriginDescriptor::MultiOrigin),
            Err(RetryModeError::NotFixedOrigin)
        ));
        assert!(matches!(
            mode.resolve(fixed("example.com")),
            Ok(ReqwestRetryInstall::Custom(_))
        ));
    }

    #[test]
    fn status_validates_hand_written_fixed_origin_metadata() {
        let mode = RetryMode::status(1, [StatusCode::BAD_GATEWAY]).expect("valid status mode");
        for authority in ["", "user@example.com", "example.com:not-a-port"] {
            assert!(matches!(
                mode.resolve(fixed(authority)),
                Err(RetryModeError::InvalidFixedOriginMetadata)
            ));
        }
        assert!(mode.resolve(fixed("example.com:8443")).is_ok());
        assert!(mode.resolve(fixed("[::1]:8443")).is_ok());
    }

    #[test]
    fn only_safe_methods_are_eligible() {
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(StatusRetryConfig::method_is_eligible(&method));
        }
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(!StatusRetryConfig::method_is_eligible(&method));
        }
    }
}
