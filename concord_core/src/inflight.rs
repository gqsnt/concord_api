use crate::error::{ApiClientError, ErrorContext};
use crate::transport::{BuiltRequest, BuiltResponse, TransportError, TransportErrorKind};
use http::{HeaderMap, StatusCode};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use tokio::sync::{Mutex, Notify};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct RequestKey(String);

impl RequestKey {
    #[inline]
    pub fn new(v: String) -> Self {
        Self(v)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn default_request_key(req: &BuiltRequest) -> RequestKey {
    let mut s = format!(
        "{} {}",
        req.meta.method,
        crate::redaction::sanitized_url_for_key(&req.url)
    );
    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|(k, v)| {
            let value = if crate::redaction::is_sensitive_name(k.as_str()) {
                crate::redaction::hashed_sensitive_value(v.to_str().unwrap_or("<non-utf8>"))
            } else {
                v.to_str().unwrap_or("<non-utf8>").to_string()
            };
            (k.as_str().to_ascii_lowercase(), value)
        })
        .collect();
    headers.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (k, v) in headers {
        s.push('|');
        s.push_str(&k);
        s.push('=');
        s.push_str(&v);
    }
    if let Some(body) = req.body.as_ref() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.hash(&mut hasher);
        s.push_str("|body=");
        s.push_str(&format!("{:x}", hasher.finish()));
    }
    if !req.extensions.auth_identities.is_empty() {
        s.push_str("|auth=");
        for identity in &req.extensions.auth_identities {
            s.push_str(identity);
            s.push(';');
        }
    }
    RequestKey::new(s)
}

pub trait InflightPolicy: Send + Sync + 'static {
    fn key_for(&self, _req: &BuiltRequest) -> Option<RequestKey> {
        None
    }
}

#[derive(Default)]
pub struct NoopInflightPolicy;

impl InflightPolicy for NoopInflightPolicy {}

#[derive(Default)]
pub struct SafeMethodInflightPolicy;

impl InflightPolicy for SafeMethodInflightPolicy {
    fn key_for(&self, req: &BuiltRequest) -> Option<RequestKey> {
        if req.body.is_some() {
            return None;
        }
        if matches!(req.meta.method, http::Method::GET | http::Method::HEAD) {
            Some(default_request_key(req))
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum SharedSendError {
    Transport {
        kind: TransportErrorKind,
        message: String,
    },
    HttpStatus {
        status: StatusCode,
        headers: HeaderMap,
        rate_limit: Option<crate::rate_limit::RateLimitResponseAction>,
    },
    Other {
        message: String,
    },
}

impl SharedSendError {
    pub fn from_api_error(err: &ApiClientError) -> Self {
        match err {
            ApiClientError::Transport { source, .. } => SharedSendError::Transport {
                kind: source.kind(),
                message: source.to_string(),
            },
            ApiClientError::HttpStatus {
                status,
                headers,
                rate_limit,
                ..
            } => SharedSendError::HttpStatus {
                status: *status,
                headers: headers.as_ref().clone(),
                rate_limit: rate_limit.as_deref().cloned(),
            },
            _ => SharedSendError::Other {
                message: err.to_string(),
            },
        }
    }

    pub fn into_api_error(self, ctx: ErrorContext) -> ApiClientError {
        match self {
            SharedSendError::Transport { kind, message } => {
                let io = std::io::Error::other(message);
                ApiClientError::Transport {
                    ctx,
                    source: TransportError::with_kind(kind, io),
                }
            }
            SharedSendError::HttpStatus {
                status,
                headers,
                rate_limit,
            } => ApiClientError::HttpStatus {
                ctx,
                status,
                headers: Box::new(headers),
                rate_limit: rate_limit.map(Box::new),
            },
            SharedSendError::Other { message } => {
                let io = std::io::Error::other(message);
                ApiClientError::Transport {
                    ctx,
                    source: TransportError::with_kind(TransportErrorKind::Other, io),
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum SharedSendResult {
    Ok(BuiltResponse),
    Err(SharedSendError),
}

#[derive(Default)]
pub struct InflightRegistry {
    inner: Mutex<HashMap<RequestKey, Arc<InflightEntry>>>,
}

enum JoinRole {
    Leader,
    Follower,
}

pub struct JoinHandle {
    key: RequestKey,
    entry: Arc<InflightEntry>,
    role: JoinRole,
}

struct InflightEntry {
    notify: Notify,
    result: Mutex<Option<SharedSendResult>>,
}

impl InflightEntry {
    fn new() -> Self {
        Self {
            notify: Notify::new(),
            result: Mutex::new(None),
        }
    }
}

impl InflightRegistry {
    pub async fn join_or_lead(&self, key: RequestKey) -> JoinHandle {
        let mut guard = self.inner.lock().await;
        if let Some(existing) = guard.get(&key) {
            return JoinHandle {
                key,
                entry: existing.clone(),
                role: JoinRole::Follower,
            };
        }
        let entry = Arc::new(InflightEntry::new());
        guard.insert(key.clone(), entry.clone());
        JoinHandle {
            key,
            entry,
            role: JoinRole::Leader,
        }
    }

    async fn complete(
        &self,
        key: &RequestKey,
        entry: &Arc<InflightEntry>,
        result: SharedSendResult,
    ) {
        {
            let mut out = entry.result.lock().await;
            *out = Some(result);
        }
        entry.notify.notify_waiters();
        let mut guard = self.inner.lock().await;
        guard.remove(key);
    }
}

impl JoinHandle {
    #[inline]
    pub fn is_leader(&self) -> bool {
        matches!(self.role, JoinRole::Leader)
    }

    pub async fn wait(self) -> SharedSendResult {
        loop {
            if let Some(done) = self.entry.result.lock().await.clone() {
                return done;
            }
            self.entry.notify.notified().await;
        }
    }

    pub async fn complete(self, registry: &InflightRegistry, result: SharedSendResult) {
        registry.complete(&self.key, &self.entry, result).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::RequestExtensions;
    use crate::cache::{CacheRequestMode, CacheSetting};
    use crate::rate_limit::RateLimitPlan;
    use crate::retry::RetrySetting;
    use crate::transport::RequestMeta;

    fn request(url: &str) -> BuiltRequest {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            http::HeaderValue::from_static("Bearer raw-secret"),
        );
        BuiltRequest {
            meta: RequestMeta {
                endpoint: "Test",
                method: http::Method::GET,
                idempotent: true,
                attempt: 0,
                page_index: 0,
            },
            url: url.parse().expect("valid url"),
            headers,
            body: None,
            timeout: None,
            retry: RetrySetting::Inherit,
            rate_limit: RateLimitPlan::new(),
            cache: CacheSetting::Off,
            cache_mode: CacheRequestMode::Default,
            cache_revalidation: None,
            extensions: RequestExtensions::default(),
        }
    }

    #[test]
    fn request_key_hashes_sensitive_url_and_header_values() {
        let key = default_request_key(&request(
            "https://example.com/items?api_key=query-secret&visible=plain",
        ));

        assert!(key.as_str().contains("visible=plain"));
        assert!(key.as_str().contains("api_key=%3Csensitive%3A"));
        assert!(key.as_str().contains("authorization=<sensitive:"));
        assert!(!key.as_str().contains("query-secret"));
        assert!(!key.as_str().contains("raw-secret"));
    }
}
