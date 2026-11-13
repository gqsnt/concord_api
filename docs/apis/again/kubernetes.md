# the Kubernetes API in Concord

Written by Claude Opus 5.5 (`claude-opus-5-5`, as the harness reports it), from the guide only (the tour, chapters 3 to 6, the grammar, the rules, the tables of chapter 8) and from what I know of the public Kubernetes API reference, on 2026-11-12. Nothing was compiled and no cluster was called. `(unsure)` marks a detail of Kubernetes that I did not check.

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
        /// A bearer token. Not set: a client certificate, or the middleware of `in_cluster`, says who you are.
        token?: Secret,
    ) {
        base server;
        headers { "authorization" = "Bearer {token}"; }
        read Json;
        problem Status;

        part selectors(label_selector?: String, field_selector?: String) {
            query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
        }
        /// A list in pages of `limit`, the next one asked with the `continue` token (a Rust keyword).
        part listing(limit?: u32, continue_token?: String, resource_version?: String, resource_version_match?: VersionMatch) {
            query {
                limit; "continue" = continue_token;
                "resourceVersion" = resource_version; "resourceVersionMatch" = resource_version_match;
            }
            // not `by_cursor`: it keeps the version, and the server refuses one beside a token (unsure)
            paginate move |call, page| page.value().next_token()
                .map(|token| call.continue_token(token).resource_version(None).resource_version_match(None));
        }
        /// A watch: an answer that does not end, one event on each line. The server ends it after
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

## Beside the macro

The data types are plain `serde` structs: `Pod`, `Deployment`, `Scale`, `DynamicObject` (`apiVersion`, `kind`, `metadata`, and the rest as a JSON map), `ObjectMeta`, `ListMeta` (`continue`, `resourceVersion`, `remainingItemCount`), `Head` (`metadata` only), `Preconditions` (`uid`, `resourceVersion`), three enums with `#[serde(other)]` (`Propagation`, `VersionMatch`, `FieldValidation`), and `Status`: `code`, `reason` (`"NotFound"`, `"Conflict"`, `"Expired"`, …), `message`, `details` (`name`, `kind`, `causes`, `retryAfterSeconds`). `Status` is the error of the API, a line of a watch, and the answer of some deletes. Two types in full, then what is not obvious.

```rust
/// A page of a list: `PodList`, `DeploymentList` and the others all have this shape.
#[derive(Deserialize)]
pub struct List<T> {
    pub metadata: ListMeta,
    #[serde(default)] pub items: Vec<T>,
}

impl<T> List<T> {
    /// The token of the next page. The last page has none, or an empty one (unsure).
    pub fn next_token(&self) -> Option<String> {
        self.metadata.continue_token.clone().filter(|token| !token.is_empty())
    }
}

impl<T: Send> concord::page::IntoItems for List<T> {
    type Item = T;
    fn into_items(self) -> Vec<T> { self.items }
    fn count(&self) -> usize { self.items.len() }
}

/// The kinds of patch. The kind is the `Content-Type`, and the body carries it.
#[derive(Clone)]
pub enum Patch {
    Json(serde_json::Value),        // RFC 6902: a list of operations
    Merge(serde_json::Value),       // RFC 7386
    Strategic(serde_json::Value),   // Kubernetes' own: lists merged by a key (not for custom resources)
    Apply(serde_json::Value),       // server-side apply: give `.field_manager(…)` too (JSON is YAML)
}

impl Patch {
    pub fn to_body(&self) -> Result<concord::Body, serde_json::Error> {
        let (value, label) = match self {
            Patch::Json(v) => (v, "application/json-patch+json"),
            Patch::Merge(v) => (v, "application/merge-patch+json"),
            Patch::Strategic(v) => (v, "application/strategic-merge-patch+json"),
            Patch::Apply(v) => (v, "application/apply-patch+yaml"),
        };
        Ok(concord::Body::new(serde_json::to_string(value)?, label))
    }

    /// A JSON patch may `add` to a list: sent twice, it adds twice.
    pub fn is_idempotent(&self) -> bool { !matches!(self, Patch::Json(_)) }
}

/// What a delete answers, with a 2xx in both cases: the object while it stops (a grace period,
/// a finalizer), or a `Status` once it is gone. Only `kind` tells them apart.
pub enum Deleted<T> { Going(T), Gone(Status) }

impl<'de, T: DeserializeOwned> Deserialize<'de> for Deleted<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(d)?;
        let gone = value.get("kind").and_then(|kind| kind.as_str()) == Some("Status");
        let read = if gone { serde_json::from_value(value).map(Deleted::Gone) }
                   else { serde_json::from_value(value).map(Deleted::Going) };
        read.map_err(serde::de::Error::custom)
    }
}
```

The watch: a reader of mine, made of the shipped `Stream`, and what it gives.

```rust
use concord::read::{Incoming, ReadError, Reader, Stream};

/// One line of a watch: `{"type": "ADDED", "object": {…}}`. An `ERROR` line holds a `Status`,
/// and the server then ends the watch.
#[derive(Deserialize)]
#[serde(tag = "type", content = "object", rename_all = "UPPERCASE")]
pub enum WatchEvent<T> { Added(T), Modified(T), Deleted(T), Bookmark(Head), Error(Status) }

/// `read WatchLines;`: the body as it comes, one `WatchEvent` on each line.
#[derive(Clone, Copy)]
pub struct WatchLines;

impl<T: DeserializeOwned + Send + 'static> Reader<Watch<T>> for WatchLines {
    async fn read(&self, body: Incoming) -> Result<Watch<T>, ReadError> {
        let bytes: concord::ByteStream = Stream.read(body).await?;
        Ok(Watch::new(bytes))                  // `new` cuts the bytes into lines (tokio-util's `LinesCodec`)
    }
}

/// What breaks a watch after its head. Not a `kubernetes::Error`: Concord makes those. (`thiserror`.)
pub enum WatchError { Line(serde_json::Error), Transport(Box<dyn std::error::Error + Send + Sync>) }

impl<T: DeserializeOwned> Watch<T> {
    /// The next event; `None` when the server ended the watch.
    pub async fn try_next(&mut self) -> Result<Option<WatchEvent<T>>, WatchError> {
        let Some(line) = self.lines.try_next().await.map_err(WatchError::Transport)? else { return Ok(None) };
        serde_json::from_slice(&line).map(Some).map_err(WatchError::Line)
    }
}
```

How a client is made, and the policy of the API:

```rust
type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A client for one cluster: its server, and the authority that signed its certificate (PEM, from
/// the kubeconfig). A client certificate goes in the same closure: `.identity(…)`.
pub fn connect(server: Base, ca_pem: &[u8]) -> Result<Kubernetes, BoxError> {
    let ca = reqwest::Certificate::from_pem(ca_pem)?;
    let http = concord::Http::with(|builder| builder.add_root_certificate(ca.clone()))?;
    Ok(Kubernetes::new(server).with_transport(http))
}

/// Inside a pod: the server from the environment, the authority and the token from the mounted files.
pub fn in_cluster() -> Result<Kubernetes, BoxError> {
    const DIR: &str = "/var/run/secrets/kubernetes.io/serviceaccount";
    let host = std::env::var("KUBERNETES_SERVICE_HOST")?;
    let port = std::env::var("KUBERNETES_SERVICE_PORT")?;
    let kube = connect(format!("https://{host}:{port}").parse()?, &std::fs::read(format!("{DIR}/ca.crt"))?)?;
    Ok(kube.with_middleware(TokenFile::new(format!("{DIR}/token"))))
}

/// The token of a pod. The kubelet renews the file (unsure: within the hour), so it is read again each minute.
pub struct TokenFile { path: String, copy: std::sync::Mutex<Option<(Instant, HeaderValue)>> }

impl concord::Middleware for TokenFile {
    async fn handle(&self, mut request: reqwest::Request, info: TryInfo, next: Next<'_>)
        -> Result<reqwest::Response, TransportError>
    {
        let value = self.current().map_err(|e| TransportError::blocked().with_source(e))?;  // `Bearer …`, sensitive
        request.headers_mut().insert("authorization", value);
        next.send(request, info).await
    }
}

/// The retry policy of this API: `….with_retry(k8s::retry())`.
pub fn retry() -> concord::Retry<Status> {
    concord::Retry::standard()
        .not_applied_on(429)                  // priority and fairness turns a request away before it runs
        .when(|tried| {                       // a `Status` may say how long to wait
            let seconds = tried.problem()?.details.as_ref()?.retry_after_seconds?;
            Some(Duration::from_secs(u64::from(seconds)))
        })
}
```

## Calling it

The crate is `k8s`; the generated module is `kubernetes`.

```rust
// 1. Inside a pod: the last twenty lines of every pod of one app, the list read page by page
let kube = k8s::in_cluster()?.with_retry(k8s::retry());
let shop = kube.core_v1().pods("shop");
let mut pods = shop.list().label_selector("app=web").limit(100).pages().items();
while let Some(pod) = pods.try_next().await? {
    let tail = shop.log(&pod.metadata.name).container("app").tail_lines(20).await?;
    println!("== {}\n{tail}", pod.metadata.name);
}

// 2. Five replicas: read the scale, change it, write it back; a 409 means that somebody wrote first
let kube = k8s::connect(server.parse()?, &ca_pem)?.token(token);
let web = kube.apps_v1().deployments("shop").scale("web");
loop {
    let mut scale = web.get().await?;
    scale.spec.replicas = 5;
    match web.replace(scale).field_manager("shop-ops").await? {
        Ok(_) => break,
        Err(conflict) => tracing::debug!(reason = ?conflict.reason, "read it again"),
    }
}

// 3. Every change to the pods of a namespace, from now on, for as long as the program runs
let pods = kube.core_v1().pods("shop");
let mut version = pods.list().limit(1).send().await?.metadata.resource_version;
loop {
    let mut events = pods.watch().resource_version(version.clone()).allow_watch_bookmarks(true).await?;
    while let Some(event) = events.try_next().await? {
        match event {
            WatchEvent::Added(pod) | WatchEvent::Modified(pod) | WatchEvent::Deleted(pod) => {
                version = pod.metadata.resource_version.clone();
                handle(pod).await?;
            }
            WatchEvent::Bookmark(head) => version = head.metadata.resource_version,
            WatchEvent::Error(status) if status.code == Some(410) => version = None,    // too old: start over
            WatchEvent::Error(status) => bail!("watch refused: {:?}", status.reason),
        }
    }                                                     // the server ended it: watch again from `version`
}

// 4. Mark a pod, first as a dry run, then for real; then delete it, only if it is still that pod
let pods = kube.core_v1().pods("shop");
let mark = Patch::Strategic(json!({ "metadata": { "labels": { "drain": "yes" } } }));
pods.patch("web-0", mark.clone()).dry_run(true).await?;                // checked by the server, written nowhere
let pod = pods.patch("web-0", mark).await?;
match pods.delete("web-0").preconditions(Preconditions::uid(&pod.metadata.uid)).await? {
    Deleted::Going(pod) => println!("stopping since {:?}", pod.metadata.deletion_timestamp),
    Deleted::Gone(_) => println!("gone"),
}
```

## Findings

| # | Where | What got in the way | What I wrote instead | Kind |
|---|---|---|---|---|
| 1 | `watch`, `WatchLines` | An `ERROR` line of a watch is a `Status` (`410` when the version is too old), sent after a `200`. Concord makes its errors at the head; a record inside a stream is the author's, and the author cannot make a `kubernetes::Error`. | `WatchEvent::Error(Status)`, an event like the others, and a second error type, `WatchError`, for bytes and lines that break. A caller of a watch handles three channels: `kubernetes::Error` for the call, `WatchError`, and the `Error` event. `e.problem()` never sees the `Status` of a watch, and `retry()` never reads its `retryAfterSeconds`. | **cannot be said** |
| 2 | the client, `connect` | The cluster's certificate authority is as much an input of this client as its `server`, but `new` cannot take it: a certificate is set on the transport. `Kubernetes::new(server)` compiles, and its first call fails on almost every cluster. | `connect(server, ca_pem)` beside, with `Http::with`; the doc comment of the client points to it. | **said with pain** |
| 3 | every `query` | Every key of Kubernetes is camelCase, and an input is snake_case. Of the 23 entries that place an input, 17 are the long form, `"labelSelector" = label_selector;`. Only `limit`, `force`, `container`, `previous`, `timestamps` and `preconditions` are short. | The long form, 17 times. The parts keep it to once per key. | **said with pain** |
| 4 | `custom` | A scope has no type parameter: a custom resource reached by `group`, `version` and `plural` can only give `DynamicObject`. A typed one (`CronTab`) is a copy of the scope with the three strings fixed. And three `String`s side by side swap with no message. | `DynamicObject`; the caller decodes it with `serde_json::from_value`. | **said with pain** |
| 5 | `all_pods` | The same resource under two parents (chapter 8.3: a part cannot hold endpoints). | `list` and `watch` written again. With the parts, each is one line. | small |
| 6 | `listing` | `continue` is a Rust keyword. | `continue_token?` and `"continue" = continue_token;`. The caller writes `.continue_token(…)`, a name that the provider does not use. | small |
| 7 | `listing`, `paginate` | `by_cursor` keeps every other input, and Kubernetes refuses a `continue` token beside a `resourceVersion` (unsure: a `400`). | A pager in place, in the part: it sets the token and un-sets the two versions. One expression. | small |
| 8 | `delete` | Two shapes under one `2xx`: the object while it stops, or a `Status`. An arm keys on the status only. | `Deleted<T>`, with a `Deserialize` that looks at `kind`. | small |
| 9 | `log`, `follow_log` | `follow=true` turns the answer from text into a stream, and the type after the arrow is fixed. | Two endpoints on one path. | small |
| 10 | `follow_log`, `watching` | Silent by nature, as long as the cluster is quiet. `idle_timeout` takes a `Duration`, and the guide does not say how to write "no limit" (is `Duration::MAX` safe for the engine's timer?). | A day for a log; `timeoutSeconds` plus a minute for a watch. | small |
| 11 | `scale`, `namespaced` | An input may not be named like an owner in reach: `replace(scale: Scale)` under `scope scale`, and `scope namespace(namespace: String)`. | `desired`, and `namespaced`. | small |
| 12 | `status`, `watch`, `patch` | Names I read twice. `scope status` holds the statement `status { }`, and gives a handle `…::Status` beside my `Status` (rule 24: mine stays mine). The endpoint `watch` gives `…::Watch` beside my `Watch<T>`. `PATCH patch` takes the input `patch` from its part, and rule 4 does not say whether an endpoint is an owner. | The provider's names, kept. | small |
| 13 | `WatchLines` | The guide does not say whether a reader of mine that hands the body on as a stream gets what `read Stream;` gets (the try ends at the head, no size limit, the clock while `try_next()` waits), nor the error type of the items of a `ByteStream`. | `Stream.read(body)` inside my reader, so that the shipped reader does what it does; the item error boxed. | small |
| 14 | `patching`, `write` | Server-side apply needs `fieldManager` (unsure: the server refuses it without one). It is an optional input of `write`, and two parts of one node cannot both write the key. | A comment on `Patch::Apply`. | small |
| 15 | `in_cluster` | Inside a pod the token is a file that the kubelet renews: a process, so a middleware. A middleware of the description is on every client and sees only required inputs, and most clients do not run in a pod. | `TokenFile`, added by `in_cluster()` with `with_middleware`. | small |

What I wished I could write:

1. `-> concord::Records<WatchEvent<Pod>> { read JsonLines.refused_when(WatchEvent::is_error); }`: each item a `kubernetes::Result`, an `ERROR` line given as `e.problem()`.
2. `pub client Kubernetes(server: Base, ca: Certificate) { base server; trust ca; … }`: the authority as an input of `new`.
3. `keys camelCase;` once on the client, then `query { label_selector; field_selector; resource_version; }`.
4. `scope custom<T: Resource>(namespace: String) { path "apis/{T::GROUP}/{T::VERSION}/namespaces/{namespace}/{T::PLURAL}"; GET list(..listing) -> List<T>; }`.

## Measures

| | |
|---|---|
| Endpoints | 18: the pods of a namespace 9, of every namespace 2, a deployment's `status` 2 and `scale` 3, custom objects 2. |
| Lines of the `api!` block | 167, from `concord::api! {` to its `}`. |
| A name of an input written again only to place it | 45, for 45 inputs in all: each input is placed once. `base` 1, holes of `path` 15, `headers` 1, entries of `query` 20, `body` 5 expressions and 3 entries. 17 of them are the long form only because the key is camelCase or a keyword. Not counted: `timeout_seconds` in `idle_timeout`, `patch` in `idempotent`. |
| Lines of Rust beside that exist only because the language could not say something | 25, doc comments included: `connect` (7), because the authority cannot be an input of `new`; the reader `WatchLines` (9), `WatchError` (2) and `try_next` (7), because a stream of records, with the API's error inside, cannot be said. `WatchEvent` is the API's data and is not counted; `Watch::new`, a line codec, is not shown and not counted. |

## Scores

- **Writing this description: 7.** Parts, `base server;`, a patch whose kind is a value, and `idle_timeout` where the API is silent all fit the first time; the camelCase keys and the error inside a watch cost the most.
- **Reading it back a month later: 7.** The tree reads as the URL, and `GET watch(..selectors, ..watching) -> Watch<Pod>;` is one line; but its reader, its silence and its `watch=true` are in a part at the top, and the pager in place needs its comment.
- **The calling code: 7.** `kube.core_v1().pods("shop").list().label_selector(…).pages().items()` reads like client-go, and `replace(…).await?` giving `Result<Scale, Status>` is the right shape for a conflict; a watch has three ways to fail, and `new` alone does not reach a real cluster.
