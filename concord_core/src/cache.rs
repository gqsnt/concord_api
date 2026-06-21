use crate::auth::{AuthIdentity, PendingAuthPlacement, PendingAuthSlot};
use crate::transport::{BuiltRequest, BuiltResponse};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

pub type CacheFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachePlan {
    pub mode: CachePlanMode,
    pub ttl: Option<Duration>,
    pub revalidate: bool,
    pub stale_on_error: bool,
}

#[allow(dead_code)]
impl CachePlan {
    #[inline]
    pub fn off() -> Self {
        Self {
            mode: CachePlanMode::Off,
            ttl: None,
            revalidate: false,
            stale_on_error: false,
        }
    }

    #[inline]
    pub fn ttl(ttl: Duration) -> Self {
        Self {
            mode: CachePlanMode::Ttl,
            ttl: Some(ttl),
            revalidate: false,
            stale_on_error: false,
        }
    }

    #[inline]
    pub fn http() -> Self {
        Self {
            mode: CachePlanMode::Http,
            ttl: None,
            revalidate: true,
            stale_on_error: false,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CachePlanMode {
    Off,
    Ttl,
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
    let mut key = format!(
        "{} {}",
        method,
        crate::redaction::sanitized_url_for_key(&req.url)
    );
    append_auth_cache_identity(&mut key, &req.extensions.pending_auth_slots);
    CacheKey::new(key)
}

pub(crate) fn auth_cache_identity_is_safe(req: &BuiltRequest) -> bool {
    req.extensions
        .pending_auth_slots
        .iter()
        .all(|slot| !matches!(slot.identity, AuthIdentity::Anonymous))
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

fn append_auth_cache_identity(key: &mut String, slots: &[PendingAuthSlot]) {
    if slots.is_empty() {
        return;
    }
    key.push_str("|auth=");
    for slot in slots {
        key.push_str("slot{");
        push_component(key, "cred", &slot.credential.id.safe_fragment());
        push_component(key, "use", slot.usage_id.as_str());
        if let Some(step_id) = slot.step_id {
            push_component(key, "step", step_id);
        }
        push_component(key, "place", placement_kind_fragment(&slot.placement));
        match &slot.placement {
            PendingAuthPlacement::Header(name) => push_component(key, "header", name.as_str()),
            PendingAuthPlacement::Query(name) => push_component(key, "query", name),
            PendingAuthPlacement::Bearer
            | PendingAuthPlacement::Basic
            | PendingAuthPlacement::Certificate => {}
        }
        if let Some(generation) = slot.generation {
            push_component(key, "gen", &generation.to_string());
        }
        push_component(key, "id", &slot.identity.safe_fragment());
        key.push('}');
    }
}

fn push_component(key: &mut String, label: &str, value: &str) {
    key.push_str(label);
    key.push(':');
    key.push_str(&value.len().to_string());
    key.push(':');
    key.push_str(value);
}

fn placement_kind_fragment(placement: &PendingAuthPlacement) -> &'static str {
    match placement {
        PendingAuthPlacement::Bearer => "bearer",
        PendingAuthPlacement::Header(_) => "header",
        PendingAuthPlacement::Query(_) => "query",
        PendingAuthPlacement::Basic => "basic",
        PendingAuthPlacement::Certificate => "certificate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        ApiKey, AuthApplicationRequest, AuthChallengePolicy, AuthIdentity, AuthPlacement,
        AuthProvenance, AuthRequirement, AuthUsageId, BasicCredential, CredentialId,
        CredentialMaterial, CredentialRef, RequestExtensions, SecretCredential,
        apply_basic_credential, apply_secret_credential,
    };
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

    #[derive(Clone)]
    struct AnonymousSecret(&'static str);

    impl CredentialMaterial for AnonymousSecret {
        fn safe_identity(&self) -> AuthIdentity {
            AuthIdentity::Anonymous
        }
    }

    impl SecretCredential for AnonymousSecret {
        fn secret_value(&self) -> &str {
            self.0
        }
    }

    fn requirement(placement: AuthPlacement) -> AuthRequirement {
        requirement_with_ids(
            placement,
            CredentialId::new("test", "credential"),
            AuthUsageId::new("test.credential"),
            None,
        )
    }

    fn requirement_with_ids(
        placement: AuthPlacement,
        credential_id: CredentialId,
        usage_id: AuthUsageId,
        step_id: Option<&'static str>,
    ) -> AuthRequirement {
        AuthRequirement {
            credential: CredentialRef { id: credential_id },
            placement,
            usage_id,
            step_id,
            provenance: AuthProvenance::default(),
            challenge: AuthChallengePolicy::Default,
        }
    }

    fn encoded(label: &str, value: &str) -> String {
        format!("{label}:{}:{value}", value.len())
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
        let mut auth_request = AuthApplicationRequest::new(&mut req.extensions);
        apply_basic_credential(
            &mut auth_request,
            &requirement(AuthPlacement::Basic),
            &BasicCredential::new("alice", "password").identity_hint("user-one"),
        )
        .expect("basic credential should apply");

        let key = default_cache_key(&req);

        assert!(key.as_str().contains("|auth="));
        assert!(key.as_str().contains(&encoded("cred", "test:credential")));
        assert!(key.as_str().contains(&encoded("place", "basic")));
        assert!(key.as_str().contains(&encoded("id", "user:user-one")));
    }

    #[test]
    fn default_key_partitions_basic_credentials_by_password_without_raw_secrets() {
        let mut old = request("https://example.com/items");
        apply_basic_credential(
            &mut AuthApplicationRequest::new(&mut old.extensions),
            &requirement(AuthPlacement::Basic),
            &BasicCredential::new("alice", "old-password"),
        )
        .expect("old basic credential should apply");
        let mut new = request("https://example.com/items");
        apply_basic_credential(
            &mut AuthApplicationRequest::new(&mut new.extensions),
            &requirement(AuthPlacement::Basic),
            &BasicCredential::new("alice", "new-password"),
        )
        .expect("new basic credential should apply");
        let mut same = request("https://example.com/items");
        apply_basic_credential(
            &mut AuthApplicationRequest::new(&mut same.extensions),
            &requirement(AuthPlacement::Basic),
            &BasicCredential::new("alice", "old-password"),
        )
        .expect("same basic credential should apply");

        let old_key = default_cache_key(&old);
        let new_key = default_cache_key(&new);
        let same_key = default_cache_key(&same);

        assert_eq!(old_key, same_key);
        assert_ne!(old_key, new_key);
        for key in [old_key.as_str(), new_key.as_str()] {
            assert!(!key.contains("alice"));
            assert!(!key.contains("old-password"));
            assert!(!key.contains("new-password"));
            assert!(!key.contains("YWxpY2U6b2xkLXBhc3N3b3Jk"));
            assert!(!key.contains("YWxpY2U6bmV3LXBhc3N3b3Jk"));
        }
    }

    #[test]
    fn default_key_partitions_query_auth_without_raw_secret() {
        let requirement = requirement(AuthPlacement::Query("api_key"));
        let mut a = request("https://example.com/items?page=1");
        apply_secret_credential(
            &mut AuthApplicationRequest::new(&mut a.extensions),
            &requirement,
            &ApiKey::new("QUERY_AUTH_SECRET_A"),
        )
        .expect("query auth A should apply");
        let mut b = request("https://example.com/items?page=1");
        apply_secret_credential(
            &mut AuthApplicationRequest::new(&mut b.extensions),
            &requirement,
            &ApiKey::new("QUERY_AUTH_SECRET_B"),
        )
        .expect("query auth B should apply");

        let key_a = default_cache_key(&a);
        let key_b = default_cache_key(&b);

        assert_ne!(key_a, key_b);
        for key in [key_a.as_str(), key_b.as_str()] {
            assert!(key.contains(&encoded("place", "query")));
            assert!(key.contains(&encoded("query", "api_key")));
            assert!(!key.contains("QUERY_AUTH_SECRET_A"));
            assert!(!key.contains("QUERY_AUTH_SECRET_B"));
        }
    }

    #[test]
    fn auth_cache_identity_components_are_unambiguously_encoded() {
        let delimiter_query = "api_key,id:user:x;place:bearer";
        let delimiter_step = "step,place:query:api_key";
        let delimiter_credential = CredentialId::new("test,place:query", "credential;id:user:x");
        let delimiter_usage = AuthUsageId::new("use,id:user:x;place:bearer");
        let delimiter_requirement = requirement_with_ids(
            AuthPlacement::Query(delimiter_query),
            delimiter_credential,
            delimiter_usage,
            Some(delimiter_step),
        );
        let mut delimiter_req = request("https://example.com/items?page=1");
        apply_secret_credential(
            &mut AuthApplicationRequest::new(&mut delimiter_req.extensions),
            &delimiter_requirement,
            &ApiKey::new("QUERY_AUTH_SECRET_DELIMITER"),
        )
        .expect("delimiter-heavy query auth should apply");
        let mut bearer_req = request("https://example.com/items?page=1");
        apply_secret_credential(
            &mut AuthApplicationRequest::new(&mut bearer_req.extensions),
            &requirement(AuthPlacement::Bearer),
            &ApiKey::new("QUERY_AUTH_SECRET_DELIMITER"),
        )
        .expect("bearer auth should apply");

        let delimiter_key = default_cache_key(&delimiter_req);
        let bearer_key = default_cache_key(&bearer_req);

        assert_ne!(delimiter_key, bearer_key);
        let delimiter_key = delimiter_key.as_str();
        assert!(delimiter_key.contains(&encoded("cred", "test,place:query:credential;id:user:x")));
        assert!(delimiter_key.contains(&encoded("use", "use,id:user:x;place:bearer")));
        assert!(delimiter_key.contains(&encoded("step", delimiter_step)));
        assert!(delimiter_key.contains(&encoded("place", "query")));
        assert!(delimiter_key.contains(&encoded("query", delimiter_query)));
        assert!(!delimiter_key.contains("QUERY_AUTH_SECRET_DELIMITER"));
    }

    #[test]
    fn anonymous_protected_identity_is_not_cache_safe() {
        let mut req = request("https://example.com/items");
        apply_secret_credential(
            &mut AuthApplicationRequest::new(&mut req.extensions),
            &requirement(AuthPlacement::Bearer),
            &AnonymousSecret("UNKNOWN_BEARER_SECRET"),
        )
        .expect("bearer credential should apply");

        assert!(!auth_cache_identity_is_safe(&req));
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
                        if let Ok(mut index) = index_for_listener.lock() {
                            remove_variant_from_index(&mut index, key.as_ref());
                        }
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
                    out.push_str(&crate::redaction::secret_fingerprint(value).to_string());
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
        ) -> Option<http::Response<()>> {
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
                .ok()?;
            *out.headers_mut() = headers;
            Some(out)
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
            let Some(policy_response) = self.response_for_policy(request, response) else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
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
            let Ok(mut index) = self.index.lock() else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
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
                let Ok(mut index) = self.index.lock() else {
                    return Some(CacheAfter::NotStored(CacheSkipReason::Backend));
                };
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
            let Some(policy_response) = self.response_for_policy(request, response) else {
                return CacheAfter::NotStored(CacheSkipReason::Backend);
            };
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

                let variants = match self.index.lock() {
                    Ok(index) => index.by_primary.get(&primary).cloned().unwrap_or_default(),
                    Err(_) => return CacheBefore::Miss,
                };
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
                if !stale_variants.is_empty()
                    && let Ok(mut index) = self.index.lock()
                {
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

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::auth::RequestExtensions;
        use crate::rate_limit::RateLimitPlan;
        use crate::retry::RetrySetting;
        use crate::transport::RequestMeta;
        use std::panic::AssertUnwindSafe;

        fn cached_request() -> BuiltRequest {
            BuiltRequest {
                meta: RequestMeta {
                    endpoint: "CachePoison",
                    method: http::Method::GET,
                    idempotent: true,
                    attempt: 0,
                    page_index: 0,
                },
                url: "https://example.com/cache".parse().expect("test url"),
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

        fn cached_response(request: &BuiltRequest) -> BuiltResponse {
            BuiltResponse {
                meta: request.meta.clone(),
                url: request.url.clone(),
                status: http::StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: bytes::Bytes::from_static(b"cached"),
                rate_limit: RateLimitPlan::new(),
            }
        }

        #[test]
        fn poisoned_cache_index_lock_returns_typed_backend_outcome() {
            let store = MokaCacheStore::default();
            let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
                let _guard = store.index.lock().expect("test cache index lock");
                panic!("poison cache index");
            }));

            let request = cached_request();
            let response = cached_response(&request);
            let outcome = store.store_response(&request, &response);

            assert!(matches!(
                outcome,
                CacheAfter::NotStored(CacheSkipReason::Backend)
            ));
        }
    }
}

#[cfg(feature = "cache-moka")]
pub use moka_backend::{MokaCacheConfig, MokaCacheStore};
