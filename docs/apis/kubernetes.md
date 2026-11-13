# the Kubernetes API in Concord
Written by GPT-6 (Codex), from the supplied Concord guide and Kubernetes public documentation, on 2026-11-12.

## The description

```rust
use concord::{json, Base, ByteStream, Json, Secret};
use concord::read::Stream;
use k8s_openapi::api::{apps::v1::Deployment, autoscaling::v1::Scale, core::v1::Pod};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{DeleteOptions, ListMeta, Status};
use serde::Deserialize;
use serde_json::Value;

concord::api! {
    pub client Kubernetes(cluster: Base, token: Secret) {
        base cluster;
        headers { "authorization" = "Bearer {token}"; "accept" = "application/json"; }
        read Json;
        problem Status;
        /// Reports the API server's Warning headers, including on each page and try.
        middleware KubeWarnings;

        part selectors(label_selector?: String, field_selector?: String) {
            query { "labelSelector" = label_selector; "fieldSelector" = field_selector; }
        }
        part page(limit?: u32, continuation?: String,
                  resource_version?: String, resource_version_match?: String) {
            query {
                limit; "continue" = continuation;
                "resourceVersion" = resource_version; "resourceVersionMatch" = resource_version_match;
            }
            paginate move |call, answer| answer.value().next_token().map(|next| {
                call.continuation(next).resource_version(None::<String>)
                    .resource_version_match(None::<String>)
            });
        }
        part watching(resource_version?: String, allow_watch_bookmarks?: bool, timeout_seconds?: u32) {
            query {
                "watch" = true; "resourceVersion" = resource_version;
                "allowWatchBookmarks" = allow_watch_bookmarks; "timeoutSeconds" = timeout_seconds;
            }
            read Stream;
        }
        part writing(dry_run?: String) {
            query { "dryRun" = dry_run; }
        }

        scope pods {
            path "api/v1/pods";
            GET list(..selectors, ..page) -> KubeList<Pod>;
            /// One connection; EOF and an ERROR record require an application decision.
            GET watch(..selectors, ..watching) -> ByteStream;
        }
        scope namespaces(namespace: String) {
            path "api/v1/namespaces/{namespace}";
            scope pods {
                path "pods";
                GET list(..selectors, ..page) -> KubeList<Pod>;
                GET get(name: String, resource_version?: String) -> Pod {
                    path "{name}";
                    query { "resourceVersion" = resource_version; }
                }
                POST create(pod: Pod, ..writing) -> Pod { body json(pod); }
                PUT replace(name: String, pod: Pod, ..writing) -> Pod {
                    path "{name}";
                    body json(pod);
                }
                /// Success acknowledges deletion; finalizers may keep the Pod alive.
                DELETE delete(name: String, options: DeleteOptions, ..writing) {
                    path "{name}";
                    body json(options);
                    idempotent false;
                }
                PATCH patch_json(name: String, patch: Vec<Value>, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/json-patch+json"; }
                    body json(patch);
                }
                PATCH patch_merge(name: String, patch: Value, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/merge-patch+json"; }
                    body json(patch);
                }
                PATCH patch_strategic(name: String, patch: Value, ..writing) -> Pod {
                    path "{name}";
                    headers { "content-type" = "application/strategic-merge-patch+json"; }
                    body json(patch);
                }
                GET watch(..selectors, ..watching) -> ByteStream;
                GET log(name: String, container?: String, follow?: bool,
                        previous?: bool, tail_lines?: i64, timestamps?: bool) -> ByteStream {
                    path "{name}/log";
                    query { container; follow; previous; "tailLines" = tail_lines; timestamps; }
                    headers { override "accept" = "text/plain"; }
                    read Stream;
                }
            }
        }
        scope deployments(namespace: String, name: String) {
            path "apis/apps/v1/namespaces/{namespace}/deployments/{name}";
            GET status() -> Deployment { path "status"; }
            GET scale() -> Scale { path "scale"; }
        }
        /// Requires the namespaced CronTab CRD from the Kubernetes documentation.
        GET get_crontab(namespace: String, name: String) -> CronTab {
            path "apis/stable.example.com/v1/namespaces/{namespace}/crontabs/{name}";
        }
    }
}
```

These are selected operations, not a complete SDK. The three PATCH entries are JSON Patch,
Merge Patch, and Strategic Merge Patch; server-side apply is outside this selection.
Content-Type explicitly overrides `json`'s body label. DELETE deliberately ignores its
success body. Its retry mark avoids deleting a replacement Pod after a name is reused;
callers should also supply UID preconditions. The routes and log options follow the
[Pod reference](https://kubernetes.io/docs/reference/kubernetes-api/core/pod-v1/).

`status` returns a complete `apps/v1` Deployment, while `scale` returns an
`autoscaling/v1` Scale, despite its `apps/v1` URL. Both are reads in this selection.
See the [Deployment reference](https://kubernetes.io/docs/reference/kubernetes-api/apps/deployment-v1/).
`stable.example.com`, `v1`, and `crontabs` are literal group, version, and plural from the
[CRD example](https://kubernetes.io/docs/tasks/extend-kubernetes/custom-resources/custom-resource-definitions/).
Every selected endpoint uses the caller's API server base, including the log proxy.

## Beside the macro

The imported wire types are Pod, Deployment, Scale, DeleteOptions, ListMeta, and Status.
`CronTab` is the author's ordinary serde model: apiVersion, kind, ObjectMeta, and a spec
with cronSpec, image, and optional replicas. Its routine declaration is omitted.
Its schema presumes that example CRD is installed; there is no discovery here.
The two full wire declarations below are KubeList and WatchRecord. Unknown object fields
are accepted. Resource versions and continuation tokens remain opaque strings.

```rust
#[derive(Deserialize)]
pub struct KubeList<T> {
    pub metadata: ListMeta,
    pub items: Vec<T>,
}
impl<T> KubeList<T> {
    pub fn next_token(&self) -> Option<String> {
        self.metadata.continue_.as_deref().filter(|s| !s.is_empty()).map(str::to_owned)
    }
}

#[derive(Deserialize)]
pub struct WatchRecord {
    #[serde(rename = "type")]
    pub event_type: String,
    pub object: Value,
}
```

The pager ends only on an absent or empty continuation, even for an empty items array.
The token carries the snapshot; subsequent requests clear resourceVersion and
resourceVersionMatch, retaining selectors and limit. Supplying a nonempty resourceVersion
alongside a continuation is invalid. A 410 ends paging with the typed Status; the caller
must restart the snapshot. See [API concepts](https://kubernetes.io/docs/reference/using-api/api-concepts/).
The metadata field spelling `continue_` is from the Rust [ListMeta model](https://docs.rs/kube/latest/kube/api/struct.ListMeta.html).

`KubeWarnings` is a public unit struct; its trivial declaration is omitted. This is a
chosen client behavior, not an authentication requirement. Bearer placement stays in the
macro. The middleware reports Warning values explicitly through tracing.

```rust
impl concord::Middleware for KubeWarnings {
    async fn handle(&self, request: reqwest::Request, info: concord::TryInfo,
                    next: concord::Next<'_>) -> Result<reqwest::Response, concord::TransportError> {
        let endpoint = info.endpoint().to_owned();
        let answer = next.send(request, info).await?;
        for value in answer.headers().get_all("warning").iter() {
            if let Ok(warning) = value.to_str() {
                tracing::warn!(%endpoint, %warning, "Kubernetes Warning");
            }
        }
        Ok(answer)
    }
}
```

Here is the complete watch adapter. It buffers bytes up to each newline, rather than
assuming an HTTP chunk is an event. Its 16 MiB per-record limit is this crate's choice.
An ERROR object's Status is a domain failure inside HTTP success; the adapter yields it
once and ends. Transport and decoding failures use a separate outer error. BOOKMARK
objects can contain only metadata, so object is a Value, not always a Pod. Unknown event
names survive for the caller. The API's [WatchEvent schema](https://github.com/kubernetes/kubernetes/blob/master/api/openapi-spec/v3/apis__events.k8s.io__v1_openapi.json)
describes the envelope and ERROR object.

```rust
use std::{error::Error, io, pin::Pin};
use futures_core::Stream as RustStream;
use futures_util::TryStreamExt;

pub type LocalError = Box<dyn Error + Send + Sync>;
pub type WatchItem = Result<WatchRecord, Status>;
pub type WatchRecords = Pin<Box<dyn RustStream<Item = Result<WatchItem, LocalError>> + Send>>;

fn decode_watch(line: &[u8]) -> Result<WatchItem, LocalError> {
    let record: WatchRecord = serde_json::from_slice(line)?;
    if record.event_type == "ERROR" {
        Ok(Err(serde_json::from_value(record.object)?))
    } else {
        Ok(Ok(record))
    }
}

pub fn watch_records(mut bytes: ByteStream) -> WatchRecords {
    Box::pin(async_stream::try_stream! {
        let mut line = Vec::new();
        while let Some(chunk) = bytes.try_next().await? {
            for byte in chunk {
                if byte == b'\n' {
                    if !line.is_empty() {
                        let item = decode_watch(&line)?;
                        let terminal = item.is_err();
                        yield item;
                        if terminal { return; }
                    }
                    line.clear();
                } else {
                    if line.len() == 16 * concord::MIB {
                        Err(io::Error::new(io::ErrorKind::InvalidData, "watch record exceeds limit"))?;
                    }
                    line.push(byte);
                }
            }
        }
        if !line.is_empty() { yield decode_watch(&line)?; }
    })
}
```

This adapter owns no reconnection loop. An EOF can be normal; an ERROR with code 410
requires a fresh list. A controller must preserve its last observed version and decide
when to resume or relist. Concord retry ends at the successful stream head.
For DELETE, use either DeleteOptions.dry_run or the query setter; precedence when both
are supplied is (unsure). A static bearer token is shown; kubeconfig and exec credential
plugins are application code. Cluster trust is configured on the transport below.

## Calling it

Four fragments inside an async application returning `Result<(), LocalError>`; the later
ones reuse `kube` from the first. Inputs cluster_url, cluster_ca_pem, and token_text come
from the application's cluster configuration, as in [Accessing clusters](https://kubernetes.io/docs/tasks/access-application-cluster/access-cluster/).
These examples are text only; no request was sent.

1. Trust the caller's cluster and enumerate a consistent inventory across namespaces.

```rust
let cluster: Base = cluster_url.parse()?;
let ca = reqwest::Certificate::from_pem(&cluster_ca_pem)?;
let http = concord::Http::with(|b| b.add_root_certificate(ca))?;
let kube = Kubernetes::new(cluster, token_text).with_transport(http);
let mut pages = kube.pods().list().label_selector("app=web").limit(500).pages();
let mut rv = None;
while let Some(page) = pages.try_next().await? {
    rv = page.metadata.resource_version;
    for pod in page.items { println!("{:?}", pod.metadata.name); }
}
let rv = rv.ok_or("list had no resourceVersion")?;
```

2. Watch that same inventory from its snapshot, keeping bookmarks distinct from Pods.

```rust
let bytes = kube.pods().watch().label_selector("app=web").resource_version(rv)
    .allow_watch_bookmarks(true).with_idle_timeout(std::time::Duration::from_secs(3600)).await?;
let mut records = watch_records(bytes);
while let Some(item) = records.try_next().await? {
    match item {
        Ok(event) if event.event_type == "BOOKMARK" => println!("bookmark: {}", event.object),
        Ok(event) if matches!(event.event_type.as_str(), "ADDED" | "MODIFIED" | "DELETED") => {
            let pod: Pod = serde_json::from_value(event.object)?;
            println!("{} {:?}", event.event_type, pod.metadata.name);
        }
        Ok(event) => eprintln!("unhandled watch kind: {}", event.event_type),
        Err(status) => { eprintln!("watch stopped: {:?}", status); break; }
    }
}
// The controller's next step is resume, or relist after 410; this fragment consumes one connection.
```

3. Preview a merge patch, then delete the observed Pod using a UID precondition.

```rust
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Preconditions;
let pods = kube.namespaces("team-a").pods();
let pod = pods.get("web-0").await?;
let patch = serde_json::json!({"metadata": {"resourceVersion": pod.metadata.resource_version,
                                          "labels": {"reviewed": "yes"}}});
let preview = pods.patch_merge("web-0", patch).dry_run("All").await?;
let uid = pod.metadata.uid.ok_or("Pod had no UID")?;
pods.delete("web-0", DeleteOptions {
    grace_period_seconds: Some(30),
    preconditions: Some(Preconditions { uid: Some(uid), ..Default::default() }),
    ..Default::default()
}).await?;
```

4. Follow a container's logs without buffering the entire response.

```rust
use tokio::io::AsyncWriteExt;
let mut output = tokio::io::stdout();
let mut log = kube.namespaces("team-a").pods().log("web-0").container("web")
    .follow(true).tail_lines(100).with_idle_timeout(std::time::Duration::from_secs(3600)).await?;
while let Some(chunk) = log.try_next().await? { output.write_all(&chunk).await?; }
```

## Findings

| # | where | what got in the way | what you wrote instead | kind |
|---|---|---|---|---|
| 1 | watch | JSON Lines framing and ERROR records have no description form; a successful head cannot make later Status records Concord errors. | `read Stream`, a bounded framing adapter, and nested application/domain errors (8-scope: streams and author error types). | cannot be said |
| 2 | watch and followed log | A healthy stream can exceed the 30-second silence limit; I reread the clock rules to see whether the endpoint could state this. | Every caller supplies `with_idle_timeout` (8-scope: endpoint silence statement deferred). | cannot be said |
| 3 | both pod parents | Parts share inputs, not endpoint subtrees; list and watch must be written under both parents. | Two scopes with the same two declarations and shared selector/page/watch parts (8-scope: small parts). | said with pain |
| 4 | continuation paging | I first reached for by_cursor, then had to account for clearing the initial resourceVersion constraints. | A closure that advances continuation and unsets two options; a helper interprets empty tokens. | said with pain |
| 5 | three PATCH declarations | Three names repeat one method, path, body placement, and return type; the patch schema is still mostly arbitrary JSON. | Separate named endpoints with explicit Content-Type, Vec<Value> for JSON Patch, Value for the other two. | said with pain |
| 6 | input placement | Selectors and resourceVersion repeatedly need Rust names and wire names; even unchanged names are mentioned twice. | Parts plus explicit query entries; continuation avoids Rust's `continue` keyword. | small |
| 7 | cluster certificate | Trust is essential to constructing this client but cannot be required by `new` in the description. | Caller installs the cluster CA using `Http::with` and `with_transport` (8-scope: HTTP settings door). | cannot be said |
| 8 | reading lists back | Declaring pagination changes the single-page GET from `.await` to `.send().await`; I had to check which spelling applies. | `.pages()` when enumerating; use each page's items directly to retain snapshot metadata. | small |

Wishes, in prose rather than proposed Concord syntax:

- #1: Describe typed watch records, their Status failures, and resumption from resourceVersion together.
- #2: State the stream's expected silence once on its endpoint.
- #3: Mount a reusable resource subtree below a namespace and at the cluster root.
- #4: Name a continuation rule that consumes initial-only resourceVersion constraints.
- #5: Describe one PATCH operation with three body types tied to their media types.
- #7: Require validated cluster trust when constructing a client, while keeping HTTP configuration replaceable.

## Measures

- Endpoints: 15 declarations: 2 across namespaces, 10 namespaced Pod operations,
  2 Deployment subresource reads, and 1 custom-resource read. Each PATCH variant counts once.
- `api!` block: 96 physical lines, from `concord::api! {` through its closing brace,
  including blank/comment lines, excluding imports and Markdown fences.
- Second mentions solely for placement: 36; inputs declared in all: 36.
  Both count the authored text, including client, scope, part, and endpoint inputs; spreads
  are not redeclarations. A path hole, base reference, query/header value, or body reference
  counts as one placement; later behavior references do not add to this measure.
- Rust beside the macro because a description form is missing: 43 physical lines
  (41 nonblank) for watch adaptation, including its envelope (37 adapter lines plus 6 envelope lines).
  There are also 18 lines at intended extension points: 13 for warning middleware and
  5 for token extraction, making 59 support lines overall. Imports, routine models,
  KubeList's declaration, and calling code are excluded; the pager closure is inside
  the macro count. Missing endpoint timeouts and trust requirements appear in calling code.
- Total file: 355 lines. Text checked against the guide; no build or API call.

## Scores

- Writing this description: **7/10** — ordinary Kubernetes routing fits, but watch behavior and the duplicated resource branches take me outside the tree.
- Reading it back a month later: **7/10** — the wire is visible, but the watch error shape and pager's option clearing need their accompanying Rust.
- The calling code: **8/10** — normal calls and paging read well; stream clocks and controller recovery remain application obligations.
