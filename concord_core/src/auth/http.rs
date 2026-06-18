use super::errors::AuthError;
use super::future::AuthFuture;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::fmt;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
pub struct AuthHttpRequest {
    pub method: Method,
    pub url: Url,
    pub headers: HeaderMap,
    pub body: Option<Bytes>,
    pub mode: AuthMode,
    pub policy: AuthInternalPolicy,
}

#[derive(Clone, Debug)]
pub struct AuthHttpResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthMode {
    SkipAuth,
    UseAuth(AuthRequirementId),
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

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthInternalPolicy {
    pub timeout: Option<Duration>,
    pub max_transport_retries: u32,
    pub use_rate_limiter: bool,
}

pub trait AuthHttpExecutor: Send + Sync {
    fn send<'a>(
        &'a self,
        req: AuthHttpRequest,
    ) -> AuthFuture<'a, Result<AuthHttpResponse, AuthError>>;
}

#[derive(Clone, Debug, Default)]
pub struct RequestExtensions {
    pub auth_identities: Vec<String>,
    pub transport_auth: Option<TransportAuth>,
    pub sensitive_query_keys: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAuth {
    ClientCertificate { identity_id: String },
}
