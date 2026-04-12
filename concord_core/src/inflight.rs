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
}

pub fn default_request_key(req: &BuiltRequest) -> RequestKey {
    let mut s = format!("{} {}", req.meta.method, sanitized_url_for_key(&req.url));
    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .map(|(k, v)| {
            let value = if is_sensitive_name(k.as_str()) {
                format!(
                    "<sensitive:{}>",
                    hash_value(v.to_str().unwrap_or("<non-utf8>"))
                )
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
            | "cookie"
            | "set-cookie"
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
                status, headers, ..
            } => SharedSendError::HttpStatus {
                status: *status,
                headers: headers.clone(),
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
            SharedSendError::HttpStatus { status, headers } => ApiClientError::HttpStatus {
                ctx,
                status,
                headers,
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
