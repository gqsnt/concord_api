use concord::http::HeaderValue;
use concord::read::{Incoming, ReadError, Reader, Stream, StreamError};
use concord::transport::{TransportError, TryInfo};
use concord::{reqwest, Base, ByteStream, Middleware, Next};
use serde::de::{DeserializeOwned, Deserializer};
use serde::Deserialize;
use std::time::Duration;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A page of a list: `PodList`, `DeploymentList` and the others all have this shape.
#[derive(Debug, Deserialize)]
pub struct List<T> {
    pub metadata: ListMeta,
    pub items: Vec<T>,
}

impl<T> List<T> {
    /// The token of the next page. The last page has none, or an empty one.
    pub fn next_token(&self) -> Option<String> {
        self.metadata.continue_token.clone().filter(|token| !token.is_empty())
    }
}

impl<T: Send> concord::page::IntoItems for List<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> {
        self.items
    }
    fn count(&self) -> usize {
        self.items.len()
    }
}

/// A patch. Its kind is its `Content-Type`, which the body carries.
#[derive(Clone, Debug)]
pub enum Patch {
    /// RFC 6902: a list of operations.
    Json(serde_json::Value),
    /// RFC 7386.
    Merge(serde_json::Value),
    /// Kubernetes' own: lists merged by a key. Not for custom resources.
    Strategic(serde_json::Value),
    /// Server-side apply: give `.field_manager(…)` too.
    Apply(serde_json::Value),
}

impl Patch {
    pub fn to_body(&self) -> Result<concord::Body, serde_json::Error> {
        let (value, content_type) = match self {
            Patch::Json(v) => (v, "application/json-patch+json"),
            Patch::Merge(v) => (v, "application/merge-patch+json"),
            Patch::Strategic(v) => (v, "application/strategic-merge-patch+json"),
            Patch::Apply(v) => (v, "application/apply-patch+yaml"),
        };
        Ok(concord::Body::new(serde_json::to_string(value)?, content_type))
    }

    /// A JSON patch may `add` to a list: sent twice, it adds twice.
    pub fn is_idempotent(&self) -> bool {
        !matches!(self, Patch::Json(_))
    }
}

/// What a delete answers, with a 2xx in both cases: the object while it stops (a grace period,
/// a finalizer), or a `Status` once it is gone. Only `kind` tells them apart.
#[derive(Debug)]
pub enum Deleted<T> {
    Going(T),
    Gone(Status),
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Deleted<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let gone = value.get("kind").and_then(|kind| kind.as_str()) == Some("Status");
        let read = if gone {
            serde_json::from_value(value).map(Deleted::Gone)
        } else {
            serde_json::from_value(value).map(Deleted::Going)
        };
        read.map_err(serde::de::Error::custom)
    }
}

/// One line of a watch: `{"type": "ADDED", "object": {…}}`. An `ERROR` line holds a `Status`,
/// and the server then ends the watch.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
pub enum WatchEvent<T> {
    Added(T),
    Modified(T),
    Deleted(T),
    Bookmark(Head),
    Error(Status),
}

/// `read WatchLines;`: the body as it comes, cut into lines.
#[derive(Clone, Copy, Debug)]
pub struct WatchLines;

impl<T: Send + 'static> Reader<Watch<T>> for WatchLines {
    async fn read(&self, body: Incoming) -> Result<Watch<T>, ReadError> {
        let bytes: ByteStream = Stream.read(body).await?;
        Ok(Watch { bytes, line: Vec::new(), item: std::marker::PhantomData })
    }
}

/// A watch: one event after another, until the server ends it.
#[derive(Debug)]
pub struct Watch<T> {
    bytes: ByteStream,
    line: Vec<u8>,
    item: std::marker::PhantomData<fn() -> T>,
}

/// What breaks a watch after its head. It is not a `kubernetes::Error`: Concord makes those.
#[derive(Debug)]
pub enum WatchError {
    Line(serde_json::Error),
    Stream(StreamError),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchError::Line(_) => f.write_str("a line of the watch is not an event"),
            WatchError::Stream(_) => f.write_str("the watch broke"),
        }
    }
}

impl std::error::Error for WatchError {}

impl<T: DeserializeOwned> Watch<T> {
    /// The next event; `None` when the server ended the watch.
    pub async fn try_next(&mut self) -> Result<Option<WatchEvent<T>>, WatchError> {
        loop {
            if let Some(end) = self.line.iter().position(|&b| b == b'\n') {
                let line: Vec<u8> = self.line.drain(..=end).collect();
                return serde_json::from_slice(&line).map(Some).map_err(WatchError::Line);
            }
            match self.bytes.try_next().await.map_err(WatchError::Stream)? {
                Some(piece) => self.line.extend_from_slice(&piece),
                None => return Ok(None),
            }
        }
    }
}

/// A client for one cluster: its server, and the authority that signed its certificate (PEM, from
/// the kubeconfig). A client certificate goes in the same closure: `.identity(…)`.
pub fn connect(server: Base, ca_pem: &[u8]) -> Result<Kubernetes, BoxError> {
    let ca = reqwest::Certificate::from_pem(ca_pem)?;
    let http = concord::Http::with(|builder| builder.add_root_certificate(ca))?;
    Ok(Kubernetes::new(server).with_transport(http))
}

/// Inside a pod: the server from the environment, the authority and the token from the files
/// that Kubernetes mounts there.
pub fn in_cluster() -> Result<Kubernetes, BoxError> {
    const DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";
    let host = std::env::var("KUBERNETES_SERVICE_HOST")?;
    let port = std::env::var("KUBERNETES_SERVICE_PORT")?;
    let ca = std::fs::read(format!("{DIR}/ca.crt"))?;
    let kube = connect(format!("https://{host}:{port}").parse()?, &ca)?;
    Ok(kube.with_middleware(TokenFile(format!("{DIR}/token"))))
}

/// The token of a pod. Kubernetes renews the file, so it is read again for each request.
pub struct TokenFile(pub String);

impl Middleware for TokenFile {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let blocked = |e: BoxError| TransportError::blocked().with_source(e);
        let token = std::fs::read_to_string(&self.0).map_err(|e| blocked(e.into()))?;
        let mut value = HeaderValue::try_from(format!("Bearer {}", token.trim()))
            .map_err(|e| blocked(e.into()))?;
        value.set_sensitive(true);
        request.headers_mut().insert("authorization", value);
        next.send(request, info).await
    }
}

/// The retry policy of this API: `.with_retry(retry())`.
pub fn retry() -> kubernetes::Retry {
    kubernetes::Retry::standard()
        .not_applied_on(429)                  // priority and fairness turns a request away before it runs
        .when(|tried| {                       // a `Status` may say how long to wait
            let seconds = tried.problem()?.details.as_ref()?.retry_after_seconds?;
            Some(Duration::from_secs(u64::from(seconds)))
        })
}
