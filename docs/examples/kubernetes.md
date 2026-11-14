# Kubernetes

*[Concord](../../README.md) · [docs](../README.md) · examples · previous: [Stripe](stripe.md) · next: [Slack](slack.md)*

This page is for a Rust developer who calls the API server of a Kubernetes cluster. It gives a client with 18 endpoints that you can copy (pods, the scale and status of a deployment, custom resources), and shows a watch that never ends, a log followed as it is written, patches whose type is given by a header, and a certificate that the connection needs.

Here is Kubernetes as Concord sees it. The server is the cluster's own (`https://10.0.0.1:6443`, or a proxy with a prefix): `base server;`, an input of type `Base`. Its certificate is signed by the cluster's own authority, so the transport must trust it: `connect(server, ca_pem)` builds the client. Authentication is a bearer token or, inside a pod, a token file that Kubernetes renews (a middleware layer reads it). Lists are paginated by a `continue` token; a watch is a response that never ends, with one JSON event on each line, read by the author's own reader. Every query key is in camelCase. Kubernetes rejects a request with a `Status` object (`code`, `reason`, `message`, `details`), which also appears as a line of a watch and as the response to some deletes. The Kubernetes reference is [kubernetes.io/docs/reference/kubernetes-api](https://kubernetes.io/docs/reference/kubernetes-api/). `(unsure)` marks a detail about Kubernetes that has not been checked.

## The description

```rust
use std::time::Duration;
use concord::read::Stream;
use concord::{json, Base, Json, Secret, Text};

concord::api! {
    /// The Kubernetes API of one cluster (a subset): pods, two subresources of a deployment, custom resources.
    /// The cluster's certificate is a fact of the connection: build the client with `connect` or `in_cluster`.
    pub client Kubernetes(
        /// The API server: `https://10.0.0.1:6443`, or a proxy with a prefix (`https://rancher.example/k8s/clusters/c-1`).
        server: Base,
        /// A bearer token. Not set: a client certificate, or the middleware of `in_cluster`, provides the credentials.
        token?: Secret,
    ) {
        base server;
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem Status;

        part selectors(label_selector?: String, field_selector?: String) {
            query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
        }
        /// A list in pages of `limit` items, the next one requested with the `continue` token (a Rust keyword).
        part listing(limit?: u32, continue_token?: String, resource_version?: String, resource_version_match?: VersionMatch) {
            query {
                limit; "continue" = continue_token;
                "resourceVersion" = resource_version; "resourceVersionMatch" = resource_version_match;
            }
            // not `by_cursor`: it keeps the version, and the server rejects one alongside a token (unsure)
            paginate move |call, page| page.value().next_token()
                .map(|token| call.continue_token(token).resource_version(None).resource_version_match(None));
        }
        /// A watch: a response that does not end, one event on each line. The server ends it after
        /// `timeout_seconds`, or after 30 to 60 minutes when it is not set (unsure).
        part watching(resource_version?: String, allow_watch_bookmarks?: bool, timeout_seconds?: u32) {
            query {
                "watch" = "true"; "resourceVersion" = resource_version;
                "allowWatchBookmarks" = allow_watch_bookmarks; "timeoutSeconds" = timeout_seconds;
            }
            read WatchLines;
            idle_timeout Duration::from_secs(u64::from(timeout_seconds.unwrap_or(3600)) + 60);
        }
        /// What every write takes.
        part write(dry_run?: bool, field_manager?: String, field_validation?: FieldValidation) {
            query {
                "dryRun" = dry_run.and_then(|on| on.then_some("All"));
                "fieldManager" = field_manager; "fieldValidation" = field_validation;
            }
        }
        /// A patch. Its kind is its `Content-Type`, which the body carries.
        part patching(patch: Patch, force?: bool) {
            body patch.to_body();
            query { force; }
            idempotent patch.is_idempotent();
        }
        part logs(
            container?: String, previous?: bool, timestamps?: bool,
            since_seconds?: i64, tail_lines?: i64, limit_bytes?: i64,
        ) {
            query {
                container; previous; timestamps;
                "sinceSeconds" = since_seconds; "tailLines" = tail_lines; "limitBytes" = limit_bytes;
            }
        }

        /// The core group.
        scope core_v1 {
            path "api/v1";

            /// The pods of one namespace.
            scope pods(namespace: String) {
                path "namespaces/{namespace}/pods";

                GET list(..selectors, ..listing) -> List<Pod>;
                GET watch(..selectors, ..watching) -> Watch<Pod>;
                POST create(pod: Pod, ..write) -> Pod { body json(pod); }

                /// `None` when there is no such pod, or no such namespace.
                GET get(name: String) -> Option<Pod> {
                    path "{name}";
                    status { 404 => None }
                }
                /// `Err`: the pod changed since the `resourceVersion` in `pod`. Read it again.
                PUT replace(name: String, pod: Pod, ..write) -> Result<Pod, Status> {
                    path "{name}";
                    body json(pod);
                    status { 409 { body } => Err(body) }
                }
                PATCH patch(name: String, ..patching, ..write) -> Pod { path "{name}"; }

                /// The pod while it stops, or a `Status` once it is gone. `preconditions`: only this `uid`,
                /// or only this version. (`fieldManager` is not a parameter of a delete: no `..write`.)
                DELETE delete(name: String, grace_period_seconds?: i64, propagation_policy?: Propagation,
                              preconditions?: Preconditions, dry_run?: bool) -> Deleted<Pod> {
                    path "{name}";
                    query { "dryRun" = dry_run.and_then(|on| on.then_some("All")); }
                    body json {
                        "gracePeriodSeconds" = grace_period_seconds;
                        "propagationPolicy" = propagation_policy;
                        preconditions;
                    }
                }

                /// The log, whole: plain text, 10 MiB at most.
                GET log(name: String, ..logs) -> String {
                    path "{name}/log";
                    read Text;
                }
                /// The log as it is written, until the container stops.
                GET follow_log(name: String, ..logs) -> concord::ByteStream {
                    path "{name}/log";
                    query { "follow" = "true"; }
                    read Stream;
                    idle_timeout Duration::from_secs(24 * 60 * 60);      // a quiet container is not a dead server
                }
            }

            /// The pods of every namespace: the same two reads, under another parent.
            scope all_pods {
                path "pods";
                GET list(..selectors, ..listing) -> List<Pod>;
                GET watch(..selectors, ..watching) -> Watch<Pod>;
            }
        }

        /// The group `apps`.
        scope apps_v1 {
            path "apis/apps/v1";

            scope deployments(namespace: String) {
                path "namespaces/{namespace}/deployments";

                /// The deployment as its controller reports it.
                scope status(name: String) {
                    path "{name}/status";
                    GET get() -> Deployment;
                    PUT replace(deployment: Deployment, ..write) -> Result<Deployment, Status> {
                        body json(deployment);
                        status { 409 { body } => Err(body) }
                    }
                }
                /// How many replicas: a `Scale` of `autoscaling/v1`.
                scope scale(name: String) {
                    path "{name}/scale";
                    GET get() -> Scale;
                    /// `desired`: an input may not be named `scale` under the scope `scale`.
                    PUT replace(desired: Scale, ..write) -> Result<Scale, Status> {
                        body json(desired);
                        status { 409 { body } => Err(body) }
                    }
                    PATCH patch(..patching, ..write) -> Scale;
                }
            }
        }

        /// The objects of a custom resource, as JSON: its group, its version, the plural of its kind.
        scope custom(group: String, version: String, plural: String) {
            path "apis/{group}/{version}";

            scope namespaced(namespace: String) {
                path "namespaces/{namespace}/{custom.plural}";

                GET list(..selectors, ..listing) -> List<DynamicObject>;
                /// `None`: no such object; also when the resource is not installed.
                GET get(name: String) -> Option<DynamicObject> {
                    path "{name}";
                    status { 404 => None }
                }
            }
        }
    }
}
```

## Beside the description

The data types are plain `serde` types, named here but not shown: `Pod`, `Deployment`, `Scale`, `DynamicObject` (`apiVersion`, `kind`, `metadata`, and the rest as a JSON map), `ObjectMeta`, `ListMeta` (`continue`, `resourceVersion`), `Head` (`metadata` only), `Preconditions` (`uid`, `resourceVersion`), three enums (`Propagation`, `VersionMatch`, `FieldValidation`), and `Status`: `code`, `reason` (`"NotFound"`, `"Conflict"`, `"Expired"`, …), `message`, `details` (with `retryAfterSeconds`).

The list and its pages, the patch, the response to a delete, the watch, how a client is built, and the API's retry policy:

```rust
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

/// What a delete returns, with a 2xx in both cases: the object while it stops (a grace period,
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

/// `read WatchLines;`: the body as it arrives, split into lines.
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
```

## Calling it

```rust
use serde_json::json;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Inside a pod: the last twenty lines of the log of every pod of one app, the list read in pages.
pub async fn tails() -> Result<(), BoxError> {
    let kube = in_cluster()?.with_retry(retry());
    let shop = kube.core_v1().pods("shop");
    let mut pods = shop.list().label_selector("app=web").limit(100).pages().items();
    while let Some(pod) = pods.try_next().await? {
        let tail = shop.log(&pod.metadata.name).container("app").tail_lines(20).await?;
        println!("== {}\n{tail}", pod.metadata.name);
    }
    Ok(())
}

/// Five replicas: read the scale, change it, write it back. A `409`: somebody wrote first.
pub async fn scale_to_five(kube: &Kubernetes) -> kubernetes::Result<()> {
    let web = kube.apps_v1().deployments("shop").scale("web");
    loop {
        let mut scale = web.get().await?;
        scale.spec.replicas = 5;
        match web.replace(scale).field_manager("shop-ops").await? {
            Ok(_) => return Ok(()),
            Err(conflict) => println!("{:?}: read it again", conflict.reason),
        }
    }
}

/// Every change to the pods of a namespace, for as long as the program runs.
pub async fn follow(kube: &Kubernetes, mut handle: impl FnMut(Pod)) -> Result<(), BoxError> {
    let pods = kube.core_v1().pods("shop");
    let mut version = pods.list().limit(1).send().await?.metadata.resource_version;
    loop {
        let mut events = pods.watch().resource_version(version.clone()).allow_watch_bookmarks(true).await?;
        while let Some(event) = events.try_next().await? {
            match event {
                WatchEvent::Added(pod) | WatchEvent::Modified(pod) | WatchEvent::Deleted(pod) => {
                    version = pod.metadata.resource_version.clone();
                    handle(pod);
                }
                WatchEvent::Bookmark(head) => version = head.metadata.resource_version,
                WatchEvent::Error(status) if status.code == Some(410) => version = None, // too old
                WatchEvent::Error(status) => return Err(format!("{:?}", status.reason).into()),
            }
        }
        // The server ended the watch: watch again from `version`.
    }
}

/// Mark a pod, first as a dry run, then for real; then delete it, only if it is still that pod.
pub async fn drain(kube: &Kubernetes) -> kubernetes::Result<()> {
    let pods = kube.core_v1().pods("shop");
    let mark = Patch::Strategic(json!({ "metadata": { "labels": { "drain": "yes" } } }));
    pods.patch("web-0", mark.clone()).dry_run(true).await?;      // checked by the server, written nowhere
    let pod = pods.patch("web-0", mark).await?;
    match pods.delete("web-0").preconditions(Preconditions::uid(&pod.metadata.uid)).await? {
        Deleted::Going(pod) => println!("stopping since {:?}", pod.metadata.deletion_timestamp),
        Deleted::Gone(_) => println!("gone"),
    }
    Ok(())
}
```

## What does not fit well, and the workaround

| Kubernetes wants | What to write |
|---|---|
| An `ERROR` line inside a watch, after a `200`: a `Status`, `410` when the version is too old. Concord builds its errors from the head of the response. | The event `WatchEvent::Error(Status)`, alongside the others, and `WatchError` for malformed bytes and lines. The caller of a watch handles three cases: the call's error, `WatchError`, and the `Error` event. |
| The cluster's certificate, as much an input as the server. `new` cannot take it: a certificate is set on the transport. | `connect(server, ca_pem)`, with `concord::Http::with(…)`. The doc comment of the client points to it. |
| Keys in camelCase, and inputs in snake_case: 17 of the 23 entries use the long form, `"labelSelector" = label_selector;`. | The long form, once for each key, in a part. |
| A custom resource, typed. A scope has no type parameter. | `DynamicObject`, decoded by the caller with `serde_json::from_value`; or one scope for each type. |
| The same resources under two parents (the pods of a namespace, the pods of every namespace). | The scope declared under each parent; the parts keep each endpoint to one line. |
| `continue`, a Rust keyword. | `continue_token?` and `"continue" = continue_token;`. |
| A next page with the token and without `resourceVersion`. `by_cursor` keeps every other input. | A pager written inline, in the part: it sets the token and unsets the two versions. |
| A delete that returns the object while it is shutting down, or a `Status` once it is gone, both with a `2xx`. | `Deleted<T>`, with a `Deserialize` that looks at `kind`. |
| `follow=true`, which turns the log from text into a stream. The return type is fixed. | Two endpoints on one path: `log` and `follow_log`. |
| A watch or a followed log, which stays silent as long as the cluster is quiet. | `idle_timeout` on the endpoint or the part: a day for a log; `timeoutSeconds` and a minute for a watch. |
| Server-side apply, which needs `fieldManager` (unsure: rejected without one). | A doc comment on `Patch::Apply`: the caller sets `.field_manager(…)`. |
