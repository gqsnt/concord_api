# Kubernetes SDK maintenance exercise (DX-01)

Binding source: `contracts/DATA_AND_VALIDATION.md` §6 (Kubernetes row) and §12 in the handoff
directory; procedure: `v4/extensions-real-apis/MAINTENANCE_BRIEF.md`. This is a deliberately
staged SDK evolution task, not a claim that Kubernetes changed. The person doing it (a Claude
agent, Opus 5.5, one session) did not write the SDK (`examples/kubernetes-sdk`) or its app;
`examples/kubernetes-sdk/FRICTION.md` was read only after stage 3 (comparison at the end).
Independence is limited: same model family as the SDK author, same campaign conventions.

## Task statement

Add a resource family using the SDK's published context/configuration, then change a selector
used by the consumer. Chosen: the **`networking.k8s.io/v1` Ingress** family (a new API group with
a dotted group name: list cluster-wide, list namespaced, get, watch cluster-wide, watch namespaced),
reached through `KubeContext` / `NamespaceContext`; the consumer's selector changes from equality
`app.kubernetes.io/part-of=shop` to set-based `app.kubernetes.io/part-of in (checkout,shop)` plus
existence `!example.com/canary`, on every list and on the watch. Evidence asked for: reused versus
duplicated code, API-group/path encoding, public extension behaviour, unchanged list/watch
machinery (pagination adapter, JSON-values watch).

Paths and shapes were checked against the pinned schema:
`https://raw.githubusercontent.com/kubernetes/kubernetes/v1.37.2/api/openapi-spec/v3/apis__networking.k8s.io__v1_openapi.json`
(retrieved 2026-11-05, 536,415 bytes, sha256
`253b343c3cae24b9c42df30842b55f54d0aed3b30d91c35aa25bf0511f6c7848`). The `apis__apps__v1` file
fetched in the same way matched the digest in `PROVIDER.md` (`e1326db2…`), so both came from the
pinned tag. Operation ids: `listNetworkingV1IngressForAllNamespaces`,
`listNetworkingV1NamespacedIngress`, `readNetworkingV1NamespacedIngress`; the namespaced list
takes the same parameters as the other lists (`allowWatchBookmarks`, `continue`, `fieldSelector`,
`labelSelector`, `limit`, `resourceVersion`, `resourceVersionMatch`, `sendInitialEvents`,
`shardSelector`, `timeoutSeconds`, `watch`). Field selectors for Ingress were checked on
<https://kubernetes.io/docs/concepts/overview/working-with-objects/field-selectors/> (retrieved
2026-11-05): only the generic `metadata.name` / `metadata.namespace`, operators `=`, `==`, `!=`,
no set-based operators.

## Commits

| Stage | Commit | Subject |
|---|---|---|
| base | `e7bfd29f59b640b4e95ef0bb0949aec877cc9ffa` | 4.4 extensions: maintenance exercise brief |
| 1 | `b2c7d0b6c0d7a5687e271ba1b7e49a892bcd5f0a` | maintenance kubernetes: frozen initial consumer |
| 2 | `f998bcf0f879ad019726814c78e8cce0c93addb6` | maintenance kubernetes: SDK change — networking.k8s.io/v1 Ingress family |
| 3 | `af786e36d60bd19afab3d607ecc6e672b69e3cd2` | maintenance kubernetes: updated consumer |

This file is added by a fourth, documentation-only commit (it records the three SHAs, so it cannot
be part of stage 3).

Base defect found on the way: at `e7bfd29` `cargo test --locked -p <any package>` failed with
`error: cannot update the lock file … because --locked was passed`, because the stub commit
`a5d5b13` added the five `checks/maintenance-*` members without lock entries. Stage 1 adds the
entries (all five, the Kubernetes one with its dependencies). Other maintenance branches will add
the same entries; the four non-Kubernetes ones are identical text.

## Commands and results

All with Rust 1.99.1, `CARGO_BUILD_JOBS=6`, in this worktree, Linux.

| When | Command | Result |
|---|---|---|
| before stage 1 (base + lock fix) | `cargo test --locked -p concord-example-kubernetes-sdk -p concord-example-kubernetes-app` | SDK: unit 7, declaration 7, decode 4, doctests 4; app: fixtures 2, ku_a 9, ku_b 13, ku_b_segmentation 2 — all pass |
| stage 1 | `cargo test --locked -p concord-check-maintenance-kubernetes` | before.rs 2 pass |
| stage 1 | `cargo clippy --locked -p concord-check-maintenance-kubernetes --all-targets -- -D warnings`, `cargo fmt --check` | clean |
| stage 2 | `cargo test --locked -p concord-example-kubernetes-sdk -p concord-example-kubernetes-app -p concord-check-maintenance-kubernetes` | SDK: unit 7, declaration **8**, decode **5**, doctests 4; app: 2 / 9 / 13 / 2 unchanged; **frozen consumer before.rs 2 pass, unedited** |
| stage 2 | clippy `-D warnings` (three packages, all targets), `cargo fmt --check`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p concord-example-kubernetes-sdk` | clean |
| stage 3 / final | `cargo test --locked -p concord-check-maintenance-kubernetes` | after.rs 4 pass, before.rs 2 pass |

## Stage 1 — the frozen consumer

`src/before.rs` (348 lines incl. docs) is a downstream "release readiness" audit using only public
exports: `KubeContext::namespace`, `NamespaceContext::{apps,core}`, the generated
`set_label_selector`, `list::{ListOperation, ListPager, ListError}`,
`concord_pagination::{sequential, Budget, Resume, Outcome}`, `watch::{open, WatchOperation,
WatchEvent, WatchOpenError}`, `concord::jsonl::JsonValuesReader`, `LabelSelector::equals`. Two
generic helpers — `collect<O: ListOperation>` (consistent chunked read, 30 lines) and
`follow<W: WatchOperation>` (one watch connection, events to a sink) — plus `audit` (Deployments
and Services per namespace) and `follow_deployments`. `tests/before.rs` asserts the exact targets:

```text
/apis/apps/v1/namespaces/team-a/deployments?labelSelector=app.kubernetes.io%2Fpart-of%3Dshop&limit=2
/apis/apps/v1/namespaces/team-a/deployments?labelSelector=app.kubernetes.io%2Fpart-of%3Dshop&limit=2&continue=synthetic-continue-team-a-1
/api/v1/namespaces/team-a/services?labelSelector=app.kubernetes.io%2Fpart-of%3Dshop&limit=2
… team-b …
/apis/apps/v1/namespaces/team-a/deployments?labelSelector=app.kubernetes.io%2Fpart-of%3Dshop&resourceVersion=5100&allowWatchBookmarks=true&timeoutSeconds=60&watch=1
```

## Stage 2 — the SDK change

`git diff --stat b2c7d0b6 f998bcf0`: 11 files, +464 / −33.

| File | +/− | What |
|---|---|---|
| `src/api.rs` | +65 / −4 | **declaration**: `scope networking_v1 { path "apis/networking.k8s.io/v1"; … scope namespaced(namespace) { path "namespaces/{namespace}"; … } }`, five endpoints. Of the added lines, 24 are parameter lines copied verbatim from the existing list (7×2) and watch (5×2) blocks, 6 are copied watch bodies (`path`/`query { "watch" = "1"; }`/`headers { ACCEPT … }`), the rest scope headers, paths, docs, imports |
| `src/model/resources.rs` | +178 / −1 | **model**: `Ingress`, `IngressSpec`, `IngressRule`, `HttpIngressRuleValue`, `HttpIngressPath`, `PathType` (open enum), `IngressBackend`, `IngressServiceBackend`, `ServiceBackendPort`, `TypedLocalObjectReference`, `IngressTls`, `IngressStatus`, `IngressLoadBalancer{Status,Ingress}`, helpers `service_backends()` / `has_address()`, `resource!(Ingress, "Ingress", "networking.k8s.io/v1", true)` |
| `src/model/mod.rs` | +8 / −3 | re-exports, `IngressList = ObjectList<Ingress>` |
| `src/context.rs` | +20 / −8 | **wrapper / published context**: `KubeContext::networking()`, `NamespaceContext::networking()`, one field, one constructor line (fields are private, so additive) |
| `src/list.rs` | +5 / −2 | **adapter registration**: two lines in the `list_operation!` invocation, import, doc count |
| `src/watch.rs` | +7 / −2 | **adapter registration**: two lines (type, item, hand-written endpoint label) in `watch_operation!`, import, doc count |
| `src/lib.rs`, `Cargo.toml`, `PROVIDER.md` | +29 / −10 | docs: layout, description, digest, 22 → 27 operations, field-selector and ingress-class notes |
| `tests/declaration.rs` | +67 / −1 | five paths, list parameters + selector encoding + redacted `continue` on the new list, two-namespace isolation |
| `tests/decode.rs` | +85 / −2 | Ingress decode (rules, backends, TLS, status, unknown `pathType`), envelope → page, watch event |

`ListPager`, `Continuation`, `into_page`, `ListError`, `watch::open`, `WatchStream`, `WatchEvent`,
`ApiError` and the selector types are **unchanged**; the new family entered the machinery through
one `Type => Item` line per generated type.

**API-group/path encoding.** The dotted group is a literal path segment
(`path "apis/networking.k8s.io/v1"`) and goes on the wire unescaped
(`/apis/networking.k8s.io/v1/namespaces/team-a/ingresses`, asserted in `tests/declaration.rs` and on
the loopback wire in `tests/after.rs`); the namespace value is encoded by Concord like every other
scope value. The group/version string now lives in three hand-maintained places with nothing tying
them together: the scope path, `Resource::API_VERSION` and the watch endpoint labels
(`"Kube.networking_v1.…"`, which follow Concord's naming convention by hand).

**Old call sites.** The frozen consumer (`src/before.rs`, `tests/before.rs`) and the whole app
(`examples/kubernetes-app`, 26 tests) compiled and passed **unchanged**; `git diff` on them is
empty. No app change was needed.

## Stage 3 — the updated consumer

`git diff --stat f998bcf0 af786e36`: 12 files, +692 / −2 (`src/after.rs` 266 lines incl. docs,
`tests/after.rs` 297, fixtures 125, `lib.rs` +4/−2).

The selector change is one function:

```rust
// before.rs
LabelSelector::equals(PART_OF, app)
// after.rs
LabelSelector::builder().in_set(PART_OF, apps).not_exists(CANARY).build()
```

Wire bytes (query value only), before → after:

```text
labelSelector=app.kubernetes.io%2Fpart-of%3Dshop
labelSelector=app.kubernetes.io%2Fpart-of+in+%28checkout%2Cshop%29%2C%21example.com%2Fcanary
```

The same `LabelSelector` value goes to the three lists and the watch; values are sorted
(`checkout,shop`) as Kubernetes renders them; an empty set is refused before any request
(`release_selector([])` is `Err(SelectorError::EmptySet)`).

The new family in the consumer:

```rust
let ingresses = collect(
    scope.networking().ingresses().set_label_selector(selector.clone()),
    request.limits,
).await.map_err(failed("ingresses"))?;
// and
follow(scope.networking().watch_ingresses(), selector, from, limits, sink).await
```

`collect` and `follow` are the frozen consumer's generic helpers, reused unchanged: the stage-3
consumer contains **zero new list or watch code**. The two-chunk Ingress chain goes through the
SDK's `ListPager` (snapshot `5202` checked on the continued chunk), the watch through
`watch::open` and ends on an `ERROR` `410` as `FollowEnd::Expired`. A `403` on the new group comes
back as `AuditError::Collect { family: "ingresses", error: CollectError::List(ListError::Api(api)) }`
with `api.endpoint() == Some("Kube.networking_v1.namespaced.ingresses")`, reason `Forbidden`,
`details.group == "networking.k8s.io"`, and neither the `Status` message (which names the service
account) nor the token in `Debug`.

What was duplicated in the consumer (consumer design, not SDK): `before::audit` builds its
selector internally from an app name, so `release_audit` repeats its per-namespace loop and error
closure (~25 lines) instead of calling it; `NamespaceRelease::unavailable` repeats
`NamespaceAudit::unavailable` (9 lines); `follow_ingresses` repeats the 20-line shape of
`follow_deployments` with one line different.

## What the caller had to learn

Generated handles and setters (`set_x` / `maybe_x`); `KubeContext` / `NamespaceContext` per-group
accessors; the SDK's `ListOperation` / `WatchOperation` traits and their `with_x(Option<_>)`
methods (a second naming scheme next to the generated `set_x`); `ListPager::fetch` as a
`sequential` factory and `ListPageMeta::snapshot`; `concord_pagination::{Budget, Resume, Outcome,
SequentialReport}`; `JsonValuesReader::max_value_bytes`; `WatchEvent` variants and
`Status::is_expired`; the `LabelSelector` builder. For the change itself, nothing new: Ingress used
exactly the concepts the Deployment path already needed, plus the Ingress model.

## Documentation read and whether it answered

| Document | Question | Answered? |
|---|---|---|
| SDK `src/lib.rs` rustdoc (there is no SDK README) | layout, how to get a namespace context | yes |
| SDK `PROVIDER.md` | pin, operation list, where paths are tested | yes; the digests let me confirm a fresh fetch came from the same tag |
| SDK `src/api.rs`, `context.rs`, `list.rs`, `watch.rs`, `model/*`, `tests/*` | how to add a family | by example only: no checklist names the six touch points (declaration, model, re-export/alias, `list_operation!`, `watch_operation!`, context) |
| app `README.md`, `src/collect.rs`, `src/list_watch.rs` | how a caller drives `ListPager` / `watch::open` | yes (the app's `collect_list` is the model for `collect`) |
| `crates/concord-pagination` crate docs, `sequential.rs` | report/outcome semantics | yes |
| Concord `docs/RECIPES.md` "An operation the SDK does not declare" | can a downstream crate add the family without editing the SDK? | yes, and it showed this SDK lacks the recommended context escape (see below) |
| Concord `docs/AUTHOR_GUIDE.md` §11 "Changing a declaration" | is adding a scope/endpoint source-compatible? | not directly: the table lists edits to existing endpoints only; additions are not mentioned (answered by compiling the frozen consumer) |
| `examples/testserver/src/lib.rs` | replies, chunked bodies, cleanup | yes |
| kubernetes.io field selectors page | Ingress field selectors | yes (above) |

## Compiler errors met

No `rustc` error in any stage: the declaration, model, context and both consumers compiled on the
first build. Two clippy lints, both mine and both pointing at the exact line and cause:

```text
error: this assertion has a constant value
  --> examples/kubernetes-sdk/tests/decode.rs:77:5
77 |     assert!(Ingress::NAMESPACED);
   = help: consider moving this into a const block: `const { assert!(..) }`
```

```text
error: the following explicit lifetimes could be elided: 'a
  --> checks/maintenance-kubernetes/src/after.rs:95:10
95 | fn named<'a>(name: &'a Option<ObjectName>) -> Option<&'a str> {
```

Probe (not met, provoked to see what a maintainer who forgets one adapter line gets): deleting
`KubeNetworkingV1NamespacedIngresses => crate::model::Ingress,` from `list.rs` leaves the **SDK
compiling** (only an `unused import` warning, and only because the import had been added; without
it, silence). The SDK tests were not run in the probe, but none of them uses `ListPager` or
`ListOperation` (grep of `tests/*.rs`), so they cannot notice. The error appears in the consumer and
points at the right cause:

```text
error[E0277]: the trait bound `KubeNetworkingV1NamespacedIngresses: ListOperation` is not satisfied
   --> checks/maintenance-kubernetes/src/after.rs:222:13
221 |           let ingresses = collect(
    |                           ------- required by a bound introduced by this call
222 | /             scope
223 | |                 .networking()
224 | |                 .ingresses()
225 | |                 .set_label_selector(selector.clone()),
    | |_____________________________________________________^ the trait `ListOperation` is not implemented for `KubeNetworkingV1NamespacedIngresses`
    = help: the following other types implement trait `ListOperation`: …
note: required by a bound in `before::collect`
```

## Casts, annotations, adapters, native escapes

- Casts / type annotations: none in either consumer.
- Adapters: none new in the consumer. In the SDK, two `list_operation!` and two `watch_operation!`
  lines (the existing adapter macros).
- One comparison helper in the consumer: `IngressServiceBackend::name` is a `String` (my stage-2
  choice) while `ObjectMeta::name` is `Option<ObjectName>`, so `after.rs` needs
  `fn named(&Option<ObjectName>) -> Option<&str>` to compare them. `ObjectName` deserializes
  without re-validation, so the field could have been `ObjectName`; changing it now would be a
  breaking field-type change. A maintainer decision made too quickly, surfaced by the consumer.
- Native escapes: none (no `into_parts`, no `send_raw` in caller code; `watch::open` uses
  `send_raw` inside the SDK as before).

## Duplicated context / encoding code

- **Declaration:** 24 parameter lines + 6 watch-body lines copied (the declaration now holds 13
  copies of the 7-line list block and 6 of the 5-line watch block). Each copy carries its own
  `#[sensitive]` on `continue`; the stage-2 declaration test asserts the redaction on the new list
  so a dropped attribute there would fail.
- **Context:** a new group means a new `namespaced(namespace)` scope header and four edits in
  `context.rs` (field, constructor line, two accessors). No encoding is duplicated: the namespace
  is formatted by Concord in the generated scope; selectors render through one `Display`.
- **Consumer:** none for list/watch/selector encoding; the duplication listed under stage 3 is the
  consumer's own structure.

## Hidden collection, metadata

- Hidden collection: none. `collect` returns the whole collection by design (the audit needs it;
  bounded by `max_chunks × chunk_size`), `follow` hands each event to the sink and retains only
  counters. The pagination crate retains one in-flight chunk.
- Metadata preserved: snapshot `resourceVersion` and chunk count per collection, the typed
  `ListError` / `ApiError` (status, reason, `details.group`, endpoint), watch counters and last
  resource version. Not preserved by the consumer: `remainingItemCount` estimates (available in
  `ListPageMeta`, not needed). Not modelled by the SDK: Deployment `spec.template` labels (so the
  audit cannot relate Services to Pods precisely), Ingress `status.loadBalancer.ingress[].ports`.

## Public extension behaviour

**Through the SDK (what this exercise did):** after the stage-2 edit the family appears exactly
where callers already look — `kube.networking()`, `ns.networking()` — and plugs into every generic
helper written against `ListOperation` / `WatchOperation` with no caller change.

**Without editing the SDK (probe, not committed; code kept verbatim below for reproduction):** a
downstream crate *can* reuse the list machinery, because `ListOperation`, `Resource`, `ObjectList`,
`ListPager` and all parameter types are public. A temporary test file in this crate (114 lines,
with `serde` added as a dev-dependency for the probe only; both removed afterwards) declared its
own `concord::api!` client `Net` with the Ingress list, a 15-line `MyIngress` model implementing
`Resource`, and a hand-written 33-line `impl ListOperation for NetNetworkingV1NamespacedIngresses`
(the SDK's `list_operation!` macro is private). It compiled and `ListPager::new(op, …)` accepted the
operation. The cost: the 7-line parameter block again, the trait forwarding by hand, and **a second
client configured from scratch**:

```rust
let net = Net::new(reqwest::Client::new())
    .with_base_url("https://k8s.example.test:6443")?                         // again
    .set_authorization(BearerToken::new("synthetic").header_value()?);      // again
```

`KubeContext` exposes no base URL, credential or `reqwest::Client` and has no native-request
builder, so the published context cannot be extended by a third party — unlike the context pattern
`docs/RECIPES.md` recommends (`examples/sdk/src/context.rs`, `tenant.request(Method::GET, &[..])`).
The only other route, preparing a declared list and rewriting its URL, is the anti-pattern the same
recipe warns against. Orphan rules also prevent a downstream crate from implementing
`ListOperation` / `WatchOperation` for SDK-generated types, which matters only for endpoints the
SDK declares without registering them.

## Friction and counter-evidence (clustered by cause)

| # | Cause / boundary | Observation | Kind |
|---|---|---|---|
| F1 | Concord declaration: no parameter groups | 30 copied lines for one family; each copy must keep `#[sensitive]` | friction (library) |
| F2 | SDK: adapter registration is manual and not checked by the SDK | forgetting a `list_operation!` line compiles and passes SDK tests; the consumer gets E0277 | friction (SDK); cheap fix: a compile-time assertion per list/watch type in the SDK tests |
| F3 | SDK: closed `KubeContext` | no third-party extension through the published context (probe) | friction (SDK design) |
| F4 | SDK + Concord: group/version in three hand-kept places (scope path, `Resource::API_VERSION`, watch labels; `Prepared` has no `endpoint()`) | nothing checks they agree | friction (small) |
| F5 | SDK: no "add a family" checklist | six touch points inferred from reading | friction (docs) |
| F6 | SDK: per-chunk factory only | every consumer writes its own `sequential` loop and outcome mapping (`before::collect` 30 lines; the app's `collect_list` ~75 lines plus its outcome types in `collect.rs`) | intentional (policy is the caller's), but duplicated across callers |
| F7 | Maintainer choice in stage 2 | `IngressServiceBackend::name: String` vs `ObjectName` | friction I introduced |
| F8 | Process | stale `Cargo.lock` at the base | not library friction |

Counter-evidence (what was easy):

- The **selector change needed no SDK change** and no knowledge of the encoding: one builder call
  differs, the wire shows the expected Kubernetes syntax, and the same value goes to lists and the
  watch. Validation (`EmptySet`, label syntax) happens before a request exists.
- The **list/watch machinery was reused unchanged**, in the SDK (one line per generated type) and in
  the consumer (zero lines): chunking, snapshot check, `410` typing, watch framing, bookmark and
  `ERROR` handling all applied to Ingresses as-is.
- The **new API group** was a literal path with dots and needed nothing special; the generated names
  (`KubeNetworkingV1Namespaced`, `Kube.networking_v1.namespaced.ingresses`) followed the
  documented pattern.
- **Old call sites compiled unchanged**, and adding a group to `NamespaceContext` is additive because
  its fields are private.
- **No rustc error** in three stages; the one provoked error (E0277) named the missing trait, the
  type and the helper whose bound required it.
- Typed errors for the new family (403 with `details.group`) came for free, redacted.

## Library or SDK changes that would remove friction (not made)

**F1 — declarable parameter group** (Concord). Before (each list today):

```rust
GET ingresses(
    query("labelSelector") label_selector?: LabelSelector,
    query("fieldSelector") field_selector?: FieldSelector,
    query limit?: u32,
    #[sensitive] query("continue") continue_token?: ContinueToken,
    query("resourceVersion") resource_version?: ResourceVersion,
    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
    query("timeoutSeconds") timeout_seconds?: u32,
) -> IngressList { path "ingresses"; }
```

After (hypothetical syntax, same generated setters):

```rust
params ListOptions { /* the seven values, once, with #[sensitive] on continue */ }
GET ingresses(use ListOptions) -> IngressList { path "ingresses"; }
```

**F3 — an extensible context** (SDK, no library change). Caller code that would become possible:

```rust
// a group the SDK does not declare, with the context's base URL, token and reqwest client
let request = kube.request(Method::GET, &["apis", "networking.k8s.io", "v1",
    "namespaces", ns.as_str(), "ingresses"])?;
```

**F2** — in the SDK tests, `fn is_list<O: ListOperation>() {}` called for every generated list type
(and the same for watches) turns a forgotten registration into an SDK compile error.

**F4** — `Prepared::endpoint() -> &'static str` (Concord) would let `watch::open` take the label from
the prepared request instead of a hand-written constant.

## Comparison with the author's `FRICTION.md` (read after stage 3)

- Confirmed: §1 (parameter repetition; my F1 — the author's worry about a forgotten `#[sensitive]`
  is real, and stage 2 added a test for it on the new list), §2 (the forwarding macro: for the SDK
  maintainer it cost one line per type; for a downstream extender it cost 33 hand-written lines
  because the macro is private), §3 (one `namespaced` scope per group: a new group added another
  one plus four context edits), §4 (hand-written watch labels; my F4), §6 (snapshot in the key:
  worked unchanged for Ingresses), §7 (adjacently tagged watch events: worked unchanged).
- Not in `FRICTION.md`, found fresh: F2 (unchecked adapter registration), F3 (closed context, no
  third-party extension), F5 (no maintainer checklist), F6 seen from a *second* consumer (each
  caller rewrites the chunk loop), F7, F8, and that the Concord compatibility table does not cover
  additions.
- Not reproduced: §10's ~40 ms per loopback request (the test server sets `TCP_NODELAY` since
  `e0950f3`; the 7-request audit test ran inside the 0.02 s reported for the whole `after.rs`
  binary).

## Limits

- Loopback test server only; no real API server was contacted. Fixtures are schema-derived
  synthetic (field names from the pinned OpenAPI documents), not captures; the server does not
  evaluate selectors, so `tests/after.rs` checks each returned object against the selector with
  `LabelSelector::matches` instead.
- One maintainer, one session, same model family as the SDK author; no population claim.
- Timings were not measured beyond the observation that each incremental test run took a few
  seconds on a warm build cache (6 jobs); they are not evidence of anything here.

## Probe source (reproduction of "Public extension behaviour")

Placed as `checks/maintenance-kubernetes/tests/zz_extension_probe.rs` with
`serde = { workspace = true, features = ["derive"] }` under `[dev-dependencies]`, run with
`cargo test -p concord-check-maintenance-kubernetes --test zz_extension_probe` (1 passed), then
removed. Essential parts:

```rust
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct MyIngress { pub metadata: ObjectMeta }

impl Resource for MyIngress {
    const KIND: &'static str = "Ingress";
    const API_VERSION: &'static str = "networking.k8s.io/v1";
    const NAMESPACED: bool = true;
    fn metadata(&self) -> &ObjectMeta { &self.metadata }
}

concord::api! {
    pub client Net(#[sensitive] header(AUTHORIZATION) authorization?: HeaderValue) {
        base "https://kubernetes.default.svc";
        format Json;
        scope networking_v1 {
            path "apis/networking.k8s.io/v1";
            scope namespaced(namespace: NamespaceName) {
                path "namespaces/{namespace}";
                GET ingresses(/* the seven list values, copied */) -> ObjectList<MyIngress> {
                    path "ingresses";
                }
            }
        }
    }
}

impl ListOperation for NetNetworkingV1NamespacedIngresses {
    type Item = MyIngress;
    // seven forwarding methods to maybe_* and send(), written by hand (33 lines)
}
```
