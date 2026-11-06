# Friction log — Kubernetes SDK and KU-A/KU-B callers

Written by the SDK author while implementing against Concord 4.2 (`concord::api!`), the new
`concord-pagination` crate and `concord::jsonl::JsonValuesReader` (integration branch at
`a501ea6`). Each entry: the provider contract, the public Concord path used, the extra code, and
whether it is **intentional flexibility** or **friction**. No library crate was edited.

## 1. Seven list parameters written out on eleven endpoints — friction

**Provider contract.** Every Kubernetes list takes the same query parameters (`labelSelector`,
`fieldSelector`, `limit`, `continue`, `resourceVersion`, `resourceVersionMatch`,
`timeoutSeconds`; plus `allowWatchBookmarks`/`watch` on watches).

**Concord path.** Each endpoint declares its own values; a scope can carry values, but these are
per request (a continue token must not be shared by a scope handle). There is no reusable
parameter group, so `src/api.rs` repeats this block eleven times (and the watch block four times):

```rust
GET pods(
    query("labelSelector") label_selector?: LabelSelector,
    query("fieldSelector") field_selector?: FieldSelector,
    query limit?: u32,
    #[sensitive] query("continue") continue_token?: ContinueToken,
    query("resourceVersion") resource_version?: ResourceVersion,
    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
    query("timeoutSeconds") timeout_seconds?: u32,
) -> PodList { path "pods"; }
```

**Extra code.** ~90 declaration lines of repetition; a forgotten `#[sensitive]` on one copy would
silently stop redacting that endpoint's token (`tests/declaration.rs` checks one endpoint; the
name-based safety net does not cover `continue`). **Proposal:** a declarable parameter set
(e.g. `params ListOptions { .. }` used as `GET pods(use ListOptions) -> PodList`) expanding to the
same values and setters. Reproduction: `examples/kubernetes-sdk/src/api.rs`.

## 2. Generic code over generated operations needs a hand-written trait per SDK — friction

**Need.** The pagination factory and the watch workflow are written once for all kinds, so they
must call `maybe_continue_token`, `maybe_limit`, `send()` on *any* of the eleven generated list
types.

**Concord path.** Generated request types are concrete structs with inherent methods; there is no
generated trait for "an endpoint with value `x: T`" or for `send()`/`prepare()`. The SDK defines
`ListOperation` / `WatchOperation` and implements them with `macro_rules!`
(`src/list.rs`, `src/watch.rs`):

```rust
macro_rules! list_operation {
    ($($op:ty => $item:ty),* $(,)?) => {$(
        impl ListOperation for $op {
            type Item = $item;
            fn with_continue(self, token: Option<ContinueToken>) -> Self {
                self.maybe_continue_token(token)
            }
            // ... five more forwarding methods ...
            fn send_list(self)
                -> impl Future<Output = Result<ObjectList<Self::Item>, concord::Error>> + Send {
                <$op>::send(self)
            }
        }
    )*};
}
list_operation! { KubeCoreV1Pods => Pod, KubeCoreV1NamespacedPods => Pod, /* 9 more */ }
```

**Extra code.** ~110 lines of forwarding. It keeps callers on the generated operations (they can
still set any value before handing the template over), which is the point. **Proposal:** a
generated `concord::Operation` trait (`type Output; fn prepare(self)`; `fn send(self)`) would
remove the `send`/`prepare` forwarding; the per-parameter forwarding is the cost of entry 1 and
would disappear with it. Not a reason for a universal endpoint trait in pagination.

## 3. The namespace scope is declared once per API group — mostly intentional

Kubernetes repeats `namespaces/{namespace}` under each group/version path, and a Concord scope has
exactly one parent, so `namespaced(namespace: NamespaceName)` is declared under `core_v1`,
`apps_v1` and `batch_v1`. `NamespaceContext` (`src/context.rs`, ~60 lines) binds one namespace
across the three generated scope handles. This mirrors the URL structure honestly; the only
friction is the triple declaration of the scope header. No change proposed.

## 4. Watches: a truthful streaming output — resolved

**Before `-> stream` outputs existed.** A typed `Decode` buffers the whole body, so the watch endpoints were
declared without `->`. Their generated `send()` therefore *drained* the watch (it returned only
when the server closed the stream, or never), and the real route was `prepare()?.send_raw()` plus
`JsonValuesReader::open_for(response, W::ENDPOINT)` with a hand-written endpoint label per watch
type (`"Kube.core_v1.namespaced.watch_pods"`) that had to match Concord's name by convention.

**Now.** Each watch is declared with its output:

```rust
GET watch_pods(/* selectors, resourceVersion, allowWatchBookmarks, timeoutSeconds */)
    -> stream WatchEvent<Pod> {
    path "pods";
    query { "watch" = "1"; }
    headers { ACCEPT = "application/json"; }
    response JsonStream;
}
```

and the ordinary caller writes
`kube.core().watch_pods().set_resource_version(rv).send().await?` and iterates events: `send()`
returns `WatchStream<Pod>` (= `concord::jsonl::JsonValues<WatchEvent<Pod>>`) as soon as the head
is accepted, without reading the body; non-2xx is a `Status` error with the response unread (read
with `KubeError::from_concord`), a refused head is `Metadata`, unread. Read errors name the
declared endpoint; the `ENDPOINT` strings and the `open_for` duplication are gone. `JsonStream`'s
defaults (standard JSON media policy, 16 MiB per event, no totals) fit Kubernetes, so the SDK
declares the static format rather than `-> stream` plus an SDK-configured reader. An application
with its own per-event bound keeps `watch::open(operation, &reader)`, which now sends the same
declared operation with `Prepared::receive()` and opens the `Received` with that reader
(`examples/kubernetes-app` does this from `WatchPolicy::max_event_bytes`).

`WatchStream` became a type alias of the reader (it was a wrapper struct forwarding `next`,
`position` and `peak_retained_bytes`): the generated output and `watch::open` now return the
same type, which also offers `completion()` and `into_stream()`. `WatchStream::from_values` is
gone (`JsonValuesReader::read(source)` is already a `WatchStream<T, S>`).

The explicit `ACCEPT = "application/json"` stays on each watch although `JsonStream` suggests the
same value: it states the provider requirement (JSON, not protobuf) independently of the format.

## 5. Reading the `Status` document of a status error — friction (small)

**Provider contract.** Failures carry a `Status` JSON document; `410` vs other statuses, and
`reason: Expired`, decide restart vs failure.

**Concord path.** `Error::into_response()` then a bounded body read and
`<Json as Decode<Status>>::decode_body((), bytes)` (`src/error.rs`). `into_response` consumes the
`concord::Error`, so the SDK error keeps the endpoint name and status code but not the original
`concord::Error`. The bounded read is hand-written:

```rust
async fn read_bounded(mut response: reqwest::Response) -> Option<Bytes> {
    let mut body = BytesMut::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if body.len() + chunk.len() > STATUS_BODY_LIMIT { return None; }
                body.extend_from_slice(&chunk);
            }
            Ok(None) => return Some(body.freeze()),
            Err(_) => return None,
        }
    }
}
```

The same loop is needed by every SDK that decodes provider error documents (the recipes say so).
**Proposal:** a public helper such as `concord::Error::read_error_body(self, limit)` returning
`(ErrorHead, Result<Bytes, ..>)` that keeps endpoint/status, or a documented recipe function the
SDKs can share.

## 6. Pagination: the consistency check lives in the key — intentional flexibility

The contract requires every chunk of a chain to report the first chunk's
`metadata.resourceVersion`. `sequential` passes only the key to the factory, and each factory
future is `'static` and owned, so there is no per-chain state to compare with. Putting the snapshot
into the key solved it without shared state, and made resume checkpoints carry their binding:

```rust
pub struct Continuation { token: ContinueToken, snapshot: ResourceVersion }
// fetch(Some(key)): send with continue=key.token, then
if let Some(expected) = expected && snapshot.as_ref() != Some(expected) {
    return Err(ListError::InconsistentSnapshot);
}
```

No friction; recorded because it is the pattern other SDKs with snapshot-bound cursors would need.
Two observations on the crate API:

- A `410 Expired` restart is a *new* `sequential` traversal (the old one has failed; nothing may
  continue it). The restart policy and its boundary record (`collect.rs`, ~70 lines) are
  application code by design (PAG: no hidden retry). Fine.
- Mapping `SequentialReport` to a typed outcome needs a `(report.outcome, failure)` match where
  some combinations cannot occur (`Stopped` without `finish` mid-flight); the code handles them
  conservatively via `report.resume`. Minor.

## 7. Watch events as an adjacently tagged serde enum — no friction

`JsonValues<WatchEvent<T>>` (`WatchStream<T>`) with `#[serde(tag = "type", content = "object")]` decodes typed
`ADDED/MODIFIED/DELETED/BOOKMARK/ERROR` events directly, in either member order. Consequence by
contract: an event with an unknown `type` is a terminal `Decode` error for that connection (the
reader is fused), so the workflow reconnects from the last resource version
(`ConnectionEnd::Malformed`) instead of skipping it. That is the intended terminal-error policy.

## 8. Cancellation is the caller's — intentional flexibility

Neither Concord nor the readers take cancellation tokens. The workflow races each await against a
caller-supplied `Future<Output = ()>` with `std::future::poll_fn` (`src/stop.rs`, ~40 lines), which
keeps the app free of a runtime dependency; dropping the future drops the response
(`ku_b_dropping_the_workflow_releases_the_watch_connection` observes the server-side close).
`Sequential::next` and `JsonValues::next` are cancel-safe, which made this straightforward.

## 9. `#[sensitive]` value types still need a raw `Display` — friction (small)

`ContinueToken` must implement `Display` as the raw token because Concord formats query values with
`Display`; `#[sensitive]` then keeps it out of Concord's own output, and the type's `Debug` is
redacted by hand. A caller who `println!("{token}")` prints it. Accepted; an opt-in
`concord::WireText` trait (used for placement instead of `Display`) would let secret-bearing types
drop `Display`. Same for the bearer token, which the SDK sidesteps by passing a sensitive native
`HeaderValue` (`authorization?: HeaderValue`), the cleanest route Concord offers.

## 10. Test infrastructure observations (not library friction)

- **~40 ms per small response on loopback.** The test server writes the response head and the
  body in separate `write_all` calls without `TCP_NODELAY`; with Nagle and delayed ACK every
  sequential request costs ~40 ms (measured: 203 requests in 8.36 s; 15 requests in 0.59 s). The
  long-chain KU-A test was therefore reduced from 200 to 60 chunks. Fix belongs in
  `examples/testserver` (set `set_nodelay(true)` on accepted streams, or write head and small
  bodies in one buffer); reported to the coordinator, not edited here.
- **`Body::Stall` cannot observe a client close** (the server task never reads or writes again), so
  "the connection was released" is proven with a heartbeat body (`Body::Generate` of `\n` every
  10 ms: JSON whitespace, no value) whose next write fails once the client is gone.

## 11. Temporary, resolved

Before the JSON values reader merged, enabling the facade's `jsonl` feature made `concord-extras`
emit dead-code warnings (the stream internals had no user), so `cargo clippy -D warnings` failed in
a dependency; `--no-deps` was used until the merge. Gone at `a501ea6`.
