use super::errors::AuthError;
use super::future::AuthFuture;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
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
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AuthInternalPolicy {
    pub timeout: Option<Duration>,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransportAuth {
    ClientCertificate { identity_id: String },
}
