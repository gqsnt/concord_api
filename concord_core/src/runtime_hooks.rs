use crate::error::ApiClientError;
use crate::transport::TransportError;
use http::{HeaderMap, Method, StatusCode};
use std::future::Future;
use std::pin::Pin;

type HookFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct HookMeta<'a> {
    pub endpoint: &'static str,
    pub method: &'a Method,
    pub url: &'a str,
    pub attempt: u32,
    pub page_index: u32,
    pub idempotent: bool,
}

#[derive(Clone, Debug)]
pub struct PreSendHookContext<'a> {
    pub meta: HookMeta<'a>,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct PostResponseHookContext<'a> {
    pub meta: HookMeta<'a>,
    pub status: StatusCode,
    pub headers: &'a HeaderMap,
}

#[derive(Debug)]
pub struct TransportErrorHookContext<'a> {
    pub meta: HookMeta<'a>,
    pub error: &'a TransportError,
}

pub trait RuntimeHooks: Send + Sync + 'static {
    fn pre_send<'a>(
        &'a self,
        _ctx: PreSendHookContext<'a>,
    ) -> HookFuture<'a, Result<(), ApiClientError>> {
        Box::pin(async { Ok(()) })
    }

    fn post_response<'a>(&'a self, _ctx: PostResponseHookContext<'a>) -> HookFuture<'a, ()> {
        Box::pin(async {})
    }

    fn transport_error<'a>(&'a self, _ctx: TransportErrorHookContext<'a>) -> HookFuture<'a, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct NoopRuntimeHooks;

impl RuntimeHooks for NoopRuntimeHooks {}
