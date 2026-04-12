use crate::transport::{BuiltRequest, BuiltResponse};
use std::future::Future;
use std::hash::{Hash, Hasher};
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
    let mut key = format!("{} {}", req.meta.method, sanitized_url_for_key(&req.url));
    append_auth_identities(&mut key, &req.extensions.auth_identities);
    CacheKey::new(key)
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

fn append_auth_identities(key: &mut String, identities: &[String]) {
    if identities.is_empty() {
        return;
    }
    key.push_str("|auth=");
    for identity in identities {
        key.push_str(identity);
        key.push(';');
    }
}

fn sanitized_url_for_key(url: &url::Url) -> String {
    if url.query().is_none() {
        return url.to_string();
    }
    let mut out = url.clone();
    out.query_pairs_mut().clear();
    {
        let mut pairs = out.query_pairs_mut();
        for (k, v) in url.query_pairs() {
            if is_sensitive_name(&k) {
                pairs.append_pair(&k, &format!("<sensitive:{}>", hash_value(&v)));
            } else {
                pairs.append_pair(&k, &v);
            }
        }
    }
    out.to_string()
}

fn is_sensitive_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "authorization"
            | "proxy-authorization"
            | "access_token"
            | "refresh_token"
            | "api_key"
            | "apikey"
            | "key"
            | "token"
            | "secret"
            | "password"
    ) || n.contains("token")
        || n.contains("secret")
        || n.contains("api-key")
        || n.contains("apikey")
        || n.ends_with("_key")
        || n.ends_with("-key")
}

fn hash_value(value: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}
