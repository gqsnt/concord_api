use super::errors::AuthError;
use super::future::AuthFuture;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::fmt;
use std::time::Duration;
use url::Url;

pub struct AuthHttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: crate::io::PreparedBody,
    pub mode: AuthMode,
    pub policy: AuthInternalPolicy,
}

impl fmt::Debug for AuthHttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthHttpRequest")
            .field("method", &self.method)
            .field(
                "url",
                &crate::redaction::sanitize_url_for_debug(&self.url, [] as [&str; 0]),
            )
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &self.body)
            .field("mode", &self.mode)
            .field("policy", &self.policy)
            .finish()
    }
}

#[derive(Clone)]
pub struct AuthHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl fmt::Debug for AuthHttpResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthHttpResponse")
            .field("status", &self.status)
            .field(
                "headers",
                &crate::debug::SanitizedHeaders::new(&self.headers),
            )
            .field("body", &format!("<{} bytes>", self.body.len()))
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthMode {
    SkipAuth,
    UseAuth {
        id: AuthRequirementId,
        requirement: crate::auth::AuthRequirement,
    },
}

impl AuthMode {
    pub fn use_auth(id: AuthRequirementId, requirement: crate::auth::AuthRequirement) -> Self {
        Self::UseAuth { id, requirement }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AuthRequirementId {
    namespace: &'static str,
    name: &'static str,
}

impl AuthRequirementId {
    #[inline]
    pub const fn new(namespace: &'static str, name: &'static str) -> Self {
        Self { namespace, name }
    }

    #[inline]
    pub fn namespace(&self) -> &'static str {
        self.namespace
    }

    #[inline]
    pub fn name(&self) -> &'static str {
        self.name
    }

    #[inline]
    pub fn safe_fragment(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
    }
}

impl fmt::Display for AuthRequirementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.name)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AuthInternalPolicy {
    pub timeout: Option<Duration>,
    pub max_transport_retries: u32,
    pub use_rate_limiter: bool,
    pub max_body_bytes: usize,
}

impl AuthInternalPolicy {
    pub const DEFAULT_MAX_BODY_BYTES: usize = 1024 * 1024;
}

impl Default for AuthInternalPolicy {
    fn default() -> Self {
        Self {
            timeout: None,
            max_transport_retries: 0,
            use_rate_limiter: false,
            max_body_bytes: Self::DEFAULT_MAX_BODY_BYTES,
        }
    }
}

pub trait AuthHttpExecutor: Send + Sync {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>>;
}
