use crate::transport::{BuiltRequest, BuiltResponse};
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::pin::Pin;
use std::time::Duration;

type CacheFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CacheKey(String);

impl CacheKey {
    #[inline]
    pub fn new(v: String) -> Self {
        Self(v)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CachePrimaryKey(String);

impl CachePrimaryKey {
    #[inline]
    pub fn new(v: String) -> Self {
        Self(v)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CacheEntryId(String);

impl CacheEntryId {
    #[inline]
    pub fn new(v: String) -> Self {
        Self(v)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CacheSetting {
    #[default]
    Inherit,
    Config(CacheConfig),
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheRequestMode {
    #[default]
    Default,
    Bypass,
    Refresh,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CacheMode {
    #[default]
    Http,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheCapacity {
    Entries(u64),
    Bytes(u64),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CacheFailureMode {
    #[default]
    Ignore,
    ServeStaleOnError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheConfig {
    pub mode: CacheMode,
    pub default_ttl: Option<Duration>,
    pub capacity: Option<CacheCapacity>,
    pub max_body_bytes: Option<usize>,
    pub revalidate: bool,
    pub shared: bool,
    pub failure_mode: CacheFailureMode,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            mode: CacheMode::Http,
            default_ttl: None,
            capacity: None,
            max_body_bytes: None,
            revalidate: true,
            shared: false,
            failure_mode: CacheFailureMode::Ignore,
        }
    }
}

impl CacheConfig {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_default_ttl(mut self, ttl: Duration) -> Self {
        self.default_ttl = Some(ttl);
        self
    }

    #[inline]
    pub fn with_http(mut self) -> Self {
        self.mode = CacheMode::Http;
        self
    }

    #[inline]
    pub fn with_capacity_entries(mut self, entries: u64) -> Self {
        self.capacity = Some(CacheCapacity::Entries(entries));
        self
    }

    #[inline]
    pub fn with_capacity_bytes(mut self, bytes: u64) -> Self {
        self.capacity = Some(CacheCapacity::Bytes(bytes));
        self
    }

    #[inline]
    pub fn with_max_body_bytes(mut self, bytes: usize) -> Self {
        self.max_body_bytes = Some(bytes);
        self
    }

    #[inline]
    pub fn with_revalidate(mut self, revalidate: bool) -> Self {
        self.revalidate = revalidate;
        self
    }

    #[inline]
    pub fn with_shared(mut self, shared: bool) -> Self {
        self.shared = shared;
        self
    }

    #[inline]
    pub fn with_failure_mode(mut self, failure_mode: CacheFailureMode) -> Self {
        self.failure_mode = failure_mode;
        self
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        true
    }
}

#[derive(Clone, Debug)]
pub enum CacheBefore {
    Miss,
    Hit(BuiltResponse),
    Revalidate {
        request_headers: http::HeaderMap,
        cached: CacheRevalidation,
    },
    Bypass,
}

#[derive(Clone, Debug)]
pub struct CacheRevalidation {
    pub key: CacheKey,
    pub cached_response: BuiltResponse,
}

#[derive(Clone, Debug)]
pub enum CacheAfter {
    Stored,
    Updated(Box<BuiltResponse>),
    NotStored(CacheSkipReason),
    Invalidated,
}

#[derive(Clone, Debug)]
pub enum CacheSkipReason {
    Disabled,
    NotCacheable,
    TooLarge,
    Backend,
}

pub fn default_cache_key(req: &BuiltRequest) -> CacheKey {
    default_cache_key_with_method(req, &req.meta.method)
}

fn default_cache_key_with_method(req: &BuiltRequest, method: &http::Method) -> CacheKey {
    let mut key = format!("{} {}", method, sanitized_url_for_key(&req.url));
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

    fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
        Box::pin(async move {
            let Some(key) = self.key_for(request) else {
                return CacheBefore::Miss;
            };
            match self.get(&key).await {
                Some(response) => CacheBefore::Hit(response),
                None => CacheBefore::Miss,
            }
        })
    }

    fn after_response<'a>(
        &'a self,
        request: &'a BuiltRequest,
        response: &'a BuiltResponse,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, CacheAfter> {
        Box::pin(async move {
            let Some(key) = self.key_for(request) else {
                return CacheAfter::NotStored(CacheSkipReason::Disabled);
            };
            self.put(key, response.clone()).await;
            CacheAfter::Stored
        })
    }

    fn after_error<'a>(
        &'a self,
        _request: &'a BuiltRequest,
        _error: &'a crate::error::ApiClientError,
        _revalidation: Option<CacheRevalidation>,
    ) -> CacheFuture<'a, Option<BuiltResponse>> {
        Box::pin(async { None })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::RequestExtensions;
    use crate::rate_limit::RateLimitPlan;
    use crate::retry::RetrySetting;
    use crate::transport::RequestMeta;

    fn request(url: &str) -> BuiltRequest {
        BuiltRequest {
            meta: RequestMeta {
                endpoint: "Test",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
            url: url.parse().expect("test url"),
            headers: http::HeaderMap::new(),
            body: None,
            timeout: None,
            retry: RetrySetting::Inherit,
            rate_limit: RateLimitPlan::new(),
            cache: CacheSetting::Config(CacheConfig::new()),
            cache_mode: CacheRequestMode::Default,
            cache_revalidation: None,
            extensions: RequestExtensions::default(),
        }
    }

    #[test]
    fn default_key_redacts_sensitive_query_values() {
        let req = request("https://example.com/items?api_key=secret&visible=plain");

        let key = default_cache_key(&req);

        assert!(key.as_str().contains("visible=plain"));
        assert!(key.as_str().contains("api_key=%3Csensitive%3A"));
        assert!(!key.as_str().contains("secret"));
    }

    #[test]
    fn default_key_includes_auth_identity() {
        let mut req = request("https://example.com/items");
        req.extensions.auth_identities.push("user:one".to_string());

        let key = default_cache_key(&req);

        assert!(key.as_str().contains("|auth=user:one;"));
    }
}

#[cfg(feature = "cache-moka")]
mod moka_backend {
    use super::*;
    use http_cache_semantics::{AfterResponse, BeforeRequest, CacheOptions, CachePolicy};
    use moka::notification::RemovalCause;
    use moka::sync::Cache;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::SystemTime;

    #[derive(Clone, Debug)]
    struct CachedEntry {
        response: BuiltResponse,
        policy: CachePolicy,
        weight: u32,
    }

    #[derive(Default)]
    struct VariantIndex {
        by_primary: HashMap<CacheKey, Vec<CacheKey>>,
        by_variant: HashMap<CacheKey, CacheKey>,
    }

    fn remove_variant_from_index(index: &mut VariantIndex, variant: &CacheKey) {
        let Some(primary) = index.by_variant.remove(variant) else {
            return;
        };
        if let Some(variants) = index.by_primary.get_mut(&primary) {
            variants.retain(|key| key != variant);
            if variants.is_empty() {
                index.by_primary.remove(&primary);
            }
        }
    }

    fn upsert_variant_index(index: &mut VariantIndex, primary: CacheKey, variant: CacheKey) {
        if let Some(previous_primary) = index.by_variant.insert(variant.clone(), primary.clone())
            && previous_primary != primary
            && let Some(previous) = index.by_primary.get_mut(&previous_primary)
        {
            previous.retain(|key| key != &variant);
            if previous.is_empty() {
                index.by_primary.remove(&previous_primary);
            }
        }
        let variants = index.by_primary.entry(primary).or_default();
        if !variants.iter().any(|key| key == &variant) {
            variants.push(variant);
        }
    }

    pub struct MokaCacheStore {
        entries: Cache<CacheKey, Arc<CachedEntry>>,
        index: Arc<Mutex<VariantIndex>>,
        max_body_bytes: usize,
        shared: bool,
    }

    impl Default for MokaCacheStore {
        fn default() -> Self {
            Self::new(MokaCacheConfig::default())
        }
    }

    #[derive(Clone, Debug)]
    pub struct MokaCacheConfig {
        pub capacity: CacheCapacity,
        pub max_body_bytes: usize,
        pub shared: bool,
    }

    impl Default for MokaCacheConfig {
        fn default() -> Self {
            Self {
                capacity: CacheCapacity::Bytes(64 * 1024 * 1024),
                max_body_bytes: 2 * 1024 * 1024,
                shared: false,
            }
        }
    }

    impl MokaCacheConfig {
        pub fn from_cache_config(config: &CacheConfig) -> Self {
            let mut out = Self::default();
            if let Some(capacity) = config.capacity {
                out.capacity = capacity;
            }
            if let Some(max_body_bytes) = config.max_body_bytes {
                out.max_body_bytes = max_body_bytes;
            }
            out.shared = config.shared;
            out
        }
    }

    impl MokaCacheStore {
        pub fn new(config: MokaCacheConfig) -> Self {
            let capacity = config.capacity;
            let max_capacity = match capacity {
                CacheCapacity::Entries(entries) => entries,
                CacheCapacity::Bytes(bytes) => bytes,
            };
            let index = Arc::new(Mutex::new(VariantIndex::default()));
            let index_for_listener = index.clone();
            let entries = Cache::builder()
                .max_capacity(max_capacity)
                .weigher(move |_key, value: &Arc<CachedEntry>| match capacity {
                    CacheCapacity::Entries(_) => 1,
                    CacheCapacity::Bytes(_) => value.weight,
                })
                .eviction_listener(
                    move |key: Arc<CacheKey>, _value: Arc<CachedEntry>, _cause: RemovalCause| {
                        let mut index = index_for_listener.lock().expect("cache index lock");
                        remove_variant_from_index(&mut index, key.as_ref());
                    },
                )
                .build();
            Self {
                entries,
                index,
                max_body_bytes: config.max_body_bytes,
                shared: config.shared,
            }
        }

        fn primary_key(&self, request: &BuiltRequest) -> Option<CacheKey> {
            match &request.cache {
                CacheSetting::Config(_) => Some(default_cache_key(request)),
                CacheSetting::Inherit | CacheSetting::Off => None,
            }
        }

        fn request_config<'a>(&self, request: &'a BuiltRequest) -> Option<&'a CacheConfig> {
            match &request.cache {
                CacheSetting::Config(config) => Some(config),
                CacheSetting::Inherit | CacheSetting::Off => None,
            }
        }

        fn variant_key(
            &self,
            primary: &CacheKey,
            request: &BuiltRequest,
            response: &BuiltResponse,
        ) -> CacheKey {
            let mut out = primary.as_str().to_string();
            out.push_str("|vary=");
            let mut vary_parts = response
                .headers
                .get(http::header::VARY)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| name.to_ascii_lowercase())
                .collect::<Vec<_>>();
            vary_parts.sort();
            for name in vary_parts {
                out.push_str(&name);
                out.push('=');
                if let Ok(header_name) = http::HeaderName::from_bytes(name.as_bytes()) {
                    let value = request
                        .headers
                        .get(&header_name)
                        .and_then(|value| value.to_str().ok())
                        .unwrap_or("");
                    out.push_str(&hash_value(value));
                }
                out.push(';');
            }
            CacheKey::new(out)
        }

        fn request_for_policy(&self, request: &BuiltRequest) -> Option<http::Request<()>> {
            let uri = request.url.as_str().parse::<http::Uri>().ok()?;
            let mut out = http::Request::builder()
                .method(request.meta.method.clone())
                .uri(uri)
                .body(())
                .ok()?;
            *out.headers_mut() = request.headers.clone();
            Some(out)
        }

        fn response_for_policy(
            &self,
            request: &BuiltRequest,
            response: &BuiltResponse,
        ) -> http::Response<()> {
            let mut headers = response.headers.clone();
            if let CacheSetting::Config(config) = &request.cache
                && let Some(ttl) = config.default_ttl
                && !headers.contains_key(http::header::CACHE_CONTROL)
                && !headers.contains_key(http::header::EXPIRES)
                && let Ok(value) =
                    http::HeaderValue::from_str(&format!("max-age={}", ttl.as_secs()))
            {
                headers.insert(http::header::CACHE_CONTROL, value);
            }

            let mut out = http::Response::builder()
                .status(response.status)
                .body(())
                .expect("valid cached response");
            *out.headers_mut() = headers;
            out
        }

        fn store_response(&self, request: &BuiltRequest, response: &BuiltResponse) -> CacheAfter {
            let Some(primary) = self.primary_key(request) else {
                return CacheAfter::NotStored(CacheSkipReason::Disabled);
            };
            let Some(config) = self.request_config(request) else {
                return CacheAfter::NotStored(CacheSkipReason::Disabled);
            };
            let max_body_bytes = config.max_body_bytes.unwrap_or(self.max_body_bytes);
            if response.body.len() > max_body_bytes {
                return CacheAfter::NotStored(CacheSkipReason::TooLarge);
            }

            let Some(policy_request) = self.request_for_policy(request) else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
            let policy_response = self.response_for_policy(request, response);
            let options = CacheOptions {
                shared: config.shared || self.shared,
                ..CacheOptions::default()
            };
            let policy = CachePolicy::new_options(
                &policy_request,
                &policy_response,
                SystemTime::now(),
                options,
            );
            if !policy.is_storable() {
                return CacheAfter::NotStored(CacheSkipReason::NotCacheable);
            }

            let variant = self.variant_key(&primary, request, response);
            let weight = response
                .body
                .len()
                .saturating_add(response.headers.len().saturating_mul(96))
                .saturating_add(512)
                .min(u32::MAX as usize) as u32;
            let entry = Arc::new(CachedEntry {
                response: response.clone(),
                policy,
                weight,
            });
            self.entries.insert(variant.clone(), entry);
            let mut index = self.index.lock().expect("cache index lock");
            upsert_variant_index(&mut index, primary, variant);
            CacheAfter::Stored
        }

        fn invalidate_after_unsafe_success(
            &self,
            request: &BuiltRequest,
            response: &BuiltResponse,
        ) -> Option<CacheAfter> {
            if matches!(request.meta.method, http::Method::GET | http::Method::HEAD) {
                return None;
            }
            if !response.status.is_success() {
                return None;
            }

            let primary = default_cache_key_with_method(request, &http::Method::GET);
            let variants = {
                let mut index = self.index.lock().expect("cache index lock");
                let variants = index.by_primary.remove(&primary);
                if let Some(variants) = variants.as_ref() {
                    for key in variants {
                        index.by_variant.remove(key);
                    }
                }
                variants
            };
            if let Some(variants) = variants {
                for key in variants {
                    self.entries.invalidate(&key);
                }
                Some(CacheAfter::Invalidated)
            } else {
                Some(CacheAfter::NotStored(CacheSkipReason::Disabled))
            }
        }

        fn revalidate_response(
            &self,
            request: &BuiltRequest,
            response: &BuiltResponse,
            revalidation: CacheRevalidation,
        ) -> CacheAfter {
            let Some(entry) = self.entries.get(&revalidation.key) else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
            let Some(policy_request) = self.request_for_policy(request) else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
            let policy_response = self.response_for_policy(request, response);
            match entry
                .policy
                .after_response(&policy_request, &policy_response, SystemTime::now())
            {
                AfterResponse::NotModified(policy, parts) => {
                    let mut updated = entry.response.clone();
                    updated.status = parts.status;
                    updated.headers = parts.headers;
                    let new_entry = Arc::new(CachedEntry {
                        response: updated.clone(),
                        policy,
                        weight: entry.weight,
                    });
                    self.entries.insert(revalidation.key, new_entry);
                    CacheAfter::Updated(Box::new(updated))
                }
                AfterResponse::Modified(_, _) => self.store_response(request, response),
            }
        }
    }

    impl CacheStore for MokaCacheStore {
        fn before_request<'a>(&'a self, request: &'a BuiltRequest) -> CacheFuture<'a, CacheBefore> {
            Box::pin(async move {
                let Some(primary) = self.primary_key(request) else {
                    return CacheBefore::Bypass;
                };
                let Some(policy_request) = self.request_for_policy(request) else {
                    return CacheBefore::Miss;
                };

                let variants = self
                    .index
                    .lock()
                    .expect("cache index lock")
                    .by_primary
                    .get(&primary)
                    .cloned()
                    .unwrap_or_default();
                let now = SystemTime::now();
                let mut stale_variants = Vec::new();
                for key in variants {
                    let Some(entry) = self.entries.get(&key) else {
                        stale_variants.push(key);
                        continue;
                    };
                    match entry.policy.before_request(&policy_request, now) {
                        BeforeRequest::Fresh(parts) => {
                            let mut response = entry.response.clone();
                            response.status = parts.status;
                            response.headers = parts.headers;
                            return CacheBefore::Hit(response);
                        }
                        BeforeRequest::Stale {
                            request: parts,
                            matches,
                        } if matches => {
                            if !self
                                .request_config(request)
                                .map(|config| config.revalidate)
                                .unwrap_or(true)
                            {
                                continue;
                            }
                            return CacheBefore::Revalidate {
                                request_headers: parts.headers,
                                cached: CacheRevalidation {
                                    key,
                                    cached_response: entry.response.clone(),
                                },
                            };
                        }
                        BeforeRequest::Stale { .. } => {}
                    }
                }
                if !stale_variants.is_empty() {
                    let mut index = self.index.lock().expect("cache index lock");
                    for key in stale_variants {
                        remove_variant_from_index(&mut index, &key);
                    }
                }
                CacheBefore::Miss
            })
        }

        fn after_response<'a>(
            &'a self,
            request: &'a BuiltRequest,
            response: &'a BuiltResponse,
            revalidation: Option<CacheRevalidation>,
        ) -> CacheFuture<'a, CacheAfter> {
            Box::pin(async move {
                if let Some(result) = self.invalidate_after_unsafe_success(request, response) {
                    return result;
                }
                if let Some(revalidation) = revalidation {
                    return self.revalidate_response(request, response, revalidation);
                }
                self.store_response(request, response)
            })
        }

        fn after_error<'a>(
            &'a self,
            request: &'a BuiltRequest,
            _error: &'a crate::error::ApiClientError,
            revalidation: Option<CacheRevalidation>,
        ) -> CacheFuture<'a, Option<BuiltResponse>> {
            Box::pin(async move {
                let serve_stale = self.request_config(request).is_some_and(|config| {
                    config.failure_mode == CacheFailureMode::ServeStaleOnError
                });
                if serve_stale {
                    revalidation.map(|cached| cached.cached_response)
                } else {
                    None
                }
            })
        }
    }
}

#[cfg(feature = "cache-moka")]
pub use moka_backend::{MokaCacheConfig, MokaCacheStore};
