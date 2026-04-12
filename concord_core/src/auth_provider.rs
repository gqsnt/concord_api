use crate::error::ApiClientError;
use crate::transport::BuiltRequest;
use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;

type AuthFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct AuthMeta<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
}

pub struct AuthPrepareContext<'a> {
    pub meta: AuthMeta<'a>,
    pub request: &'a mut BuiltRequest,
}

#[derive(Clone, Debug)]
pub struct AuthResponseContext<'a> {
    pub meta: AuthMeta<'a>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
}

pub trait AuthProvider: Send + Sync + 'static {
    fn prepare_request<'a>(
        &'a self,
        _ctx: AuthPrepareContext<'a>,
    ) -> AuthFuture<'a, Result<(), ApiClientError>> {
        Box::pin(async { Ok(()) })
    }

    fn on_response<'a>(&'a self, _ctx: AuthResponseContext<'a>) -> AuthFuture<'a, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct NoopAuthProvider;

impl AuthProvider for NoopAuthProvider {}

