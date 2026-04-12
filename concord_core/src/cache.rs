use crate::transport::{BuiltRequest, BuiltResponse};
use std::future::Future;
use std::pin::Pin;

type CacheFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    #[inline]
    pub fn new(v: String) -> Self {
        Self(v)
    }
}

pub fn default_cache_key(req: &BuiltRequest) -> CacheKey {
    CacheKey::new(format!("{} {}", req.meta.method, req.url))
}

pub trait CacheStore: Send + Sync + 'static {
    fn key_for(&self, _request: &BuiltRequest) -> Option<CacheKey> {
        None
    }

    fn get<'a>(&'a self, _key: &'a CacheKey) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async { None })
    }

    fn put<'a>(&'a self, _key: CacheKey, _response: BuiltResponse) -> CacheFuture<'a, ()> {
        Box::pin(async {})
    }
}

#[derive(Default)]
pub struct NoopCacheStore;

impl CacheStore for NoopCacheStore {}

