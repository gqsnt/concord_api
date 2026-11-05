# Kubernetes provider pin

This SDK is a **bounded, read-only slice** of the Kubernetes API. It is not a complete Kubernetes
SDK.

## Pinned server / schema pair

| Item | Value |
|---|---|
| Server release | Kubernetes **v1.37.2** (`dl.k8s.io/release/stable.txt` on the retrieval date; GitHub release published 2026-10-28) |
| Release tag | `v1.37.2` → tag object `1755f07c19a1d4a25955616aabdfdd66e0fbe224` → commit `c50e5395f1ae2d4a2c05f28fdcc38c0916cc1a32` |
| Schema documents | OpenAPI v3 under `api/openapi-spec/v3/` at that tag |
| `api__v1_openapi.json` | sha256 `d09ab224a98fb9c0e7fd128b6f395f66b23d43b6635272109dcaaafc4a3dd9c9` (2,135,483 bytes) |
| `apis__apps__v1_openapi.json` | sha256 `e1326db2904fd6356d2f16e6179f8ad8e4800c0112cb74ab3b21da7286df4dbe` (934,942 bytes) |
| `apis__batch__v1_openapi.json` | sha256 `39e41a47689d58ed0ba29c7e576fd88a47917b5c00dd2efaab71c9cf90a6b5f7` (551,273 bytes) |
| `apis__networking.k8s.io__v1_openapi.json` | sha256 `253b343c3cae24b9c42df30842b55f54d0aed3b30d91c35aa25bf0511f6c7848` (536,415 bytes; retrieved 2026-11-05 for the Ingress family, same URL pattern; the apps/v1 file fetched alongside matched the digest above, so both came from the same tag) |
| `version_openapi.json` | sha256 `9077133c6af1c993f466a91f9f870daaee779581adc2c3614176c273dd2f52a2` |
| Retrieval date | 2026-11-04 (`https://raw.githubusercontent.com/kubernetes/kubernetes/v1.37.2/api/openapi-spec/v3/<file>`) |
| Semantics source | [K1] <https://kubernetes.io/docs/reference/using-api/api-concepts/> (sections "Efficient detection of changes", "Watch bookmarks", "Retrieving large results sets in chunks", "Resource versions"), retrieved 2026-11-04. The page itself mentions v1.37 behaviour ("Kubernetes 1.37 serves most recent reads from the watch cache"). Website source repository: [O3] <https://github.com/kubernetes/website> |

The schema files are not vendored (2.1 MB for core/v1 alone); their digests above identify the
exact bytes the declaration and models were checked against. `PINNED_SERVER_VERSION` in the crate
is the same string. Resource paths and parameter names are asserted in
`tests/declaration.rs`; model field names in `tests/decode.rs` and in the app's fixtures.

The `core/v1`, `apps/v1`, `batch/v1` and `networking.k8s.io/v1` resources used here are GA and their used fields have
been stable for many releases; the pin records what was checked, not a claim that other servers
differ.

## Supported operations (27, all `GET`)

| Family | Operation (OpenAPI `operationId`) | Path |
|---|---|---|
| version | `getCodeVersion` | `GET /version/` |
| Namespaces | `listCoreV1Namespace` | `GET /api/v1/namespaces` |
| | `readCoreV1Namespace` | `GET /api/v1/namespaces/{name}` |
| Pods | `listCoreV1PodForAllNamespaces` | `GET /api/v1/pods` |
| | `listCoreV1NamespacedPod` | `GET /api/v1/namespaces/{namespace}/pods` |
| | `readCoreV1NamespacedPod` | `GET /api/v1/namespaces/{namespace}/pods/{name}` |
| | watch, all namespaces (list path, `watch=1`) | `GET /api/v1/pods?watch=1` |
| | watch, namespaced (list path, `watch=1`) | `GET /api/v1/namespaces/{namespace}/pods?watch=1` |
| Services | `listCoreV1ServiceForAllNamespaces` | `GET /api/v1/services` |
| | `listCoreV1NamespacedService` | `GET /api/v1/namespaces/{namespace}/services` |
| | `readCoreV1NamespacedService` | `GET /api/v1/namespaces/{namespace}/services/{name}` |
| ConfigMaps | `listCoreV1ConfigMapForAllNamespaces` | `GET /api/v1/configmaps` |
| | `listCoreV1NamespacedConfigMap` | `GET /api/v1/namespaces/{namespace}/configmaps` |
| | `readCoreV1NamespacedConfigMap` | `GET /api/v1/namespaces/{namespace}/configmaps/{name}` |
| Deployments | `listAppsV1DeploymentForAllNamespaces` | `GET /apis/apps/v1/deployments` |
| | `listAppsV1NamespacedDeployment` | `GET /apis/apps/v1/namespaces/{namespace}/deployments` |
| | `readAppsV1NamespacedDeployment` | `GET /apis/apps/v1/namespaces/{namespace}/deployments/{name}` |
| | watch, all namespaces | `GET /apis/apps/v1/deployments?watch=1` |
| | watch, namespaced | `GET /apis/apps/v1/namespaces/{namespace}/deployments?watch=1` |
| Jobs | `listBatchV1JobForAllNamespaces` | `GET /apis/batch/v1/jobs` |
| | `listBatchV1NamespacedJob` | `GET /apis/batch/v1/namespaces/{namespace}/jobs` |
| | `readBatchV1NamespacedJob` | `GET /apis/batch/v1/namespaces/{namespace}/jobs/{name}` |
| Ingresses | `listNetworkingV1IngressForAllNamespaces` | `GET /apis/networking.k8s.io/v1/ingresses` |
| | `listNetworkingV1NamespacedIngress` | `GET /apis/networking.k8s.io/v1/namespaces/{namespace}/ingresses` |
| | `readNetworkingV1NamespacedIngress` | `GET /apis/networking.k8s.io/v1/namespaces/{namespace}/ingresses/{name}` |
| | watch, all namespaces | `GET /apis/networking.k8s.io/v1/ingresses?watch=1` |
| | watch, namespaced | `GET /apis/networking.k8s.io/v1/namespaces/{namespace}/ingresses?watch=1` |

List parameters declared on every list: `labelSelector`, `fieldSelector`, `limit`, `continue`
(`#[sensitive]`), `resourceVersion`, `resourceVersionMatch`, `timeoutSeconds`. Watch parameters:
`labelSelector`, `fieldSelector`, `resourceVersion`, `allowWatchBookmarks` (initialized `true`),
`timeoutSeconds`, plus the literal `watch=1`.

## Deliberately unsupported

- **Every mutation** (create, update, patch, server-side apply, delete, delete-collection,
  eviction, scale) — the workflows are read-only and production mutations are not authorized.
- Subresources: `status` (including `readNetworkingV1NamespacedIngressStatus`), `log`, `exec`,
  `attach`, `portforward`, `proxy`, `scale`, `ephemeralcontainers`.
- The other `networking.k8s.io/v1` kinds (IngressClass, NetworkPolicy, IPAddress, ServiceCIDR).
- The deprecated `/api/v1/watch/...` path prefix (watches use `?watch=1` on the list path).
- `sendInitialEvents=true` streaming lists (the workflow lists first, then watches; a bookmark's
  `k8s.io/initial-events-end` annotation is decoded but no streaming-list flow is offered).
- `shardSelector` / `metadata.shardInfo` (alpha `ShardedListAndWatch`).
- Protobuf, CBOR (`application/cbor`, `application/cbor-seq`), YAML and `Table` responses.
- The inconsistent continuation token a `410 Expired` list answer may carry
  (`Status.metadata.continue`): detected and reported, never used, because it mixes snapshots.
- Authentication other than a bearer token (client certificates and exec plugins are the
  caller's `reqwest::Client` / token source); kubeconfig parsing.
- Discovery (`/api`, `/apis`, `/apis/{group}/{version}`), API aggregation, CRDs.

## Documented discrepancies and decisions

1. **`/version` vs `/version/`.** `version_openapi.json` declares the path `/version/` (trailing
   slash); `client-go`'s discovery client requests `/version`. The SDK follows the pinned schema
   (`version/` on the endpoint). The difference is not exercised against a real server here.
2. **Watch media type.** The concepts page shows a watch answered as `Content-Type:
   application/json` with a "series of JSON documents"; the OpenAPI response map also lists
   `application/json;stream=watch`. The watch is decoded as consecutive JSON values (not as a
   line protocol and not as SSE). The request sends `Accept: application/json`.
3. **`remainingItemCount`.** The schema says it is absent with label/field selectors and on the
   last chunk and that clients "should not rely on [it] to be set or to be exact"; it is reported
   and never used for planning.
4. **Continue + resourceVersion.** Per the list semantics table, `continue` with a specific
   `resourceVersion` is `400`; continued requests clear `resourceVersion`/`resourceVersionMatch`.
5. **Expired continue reason.** The `continue` parameter text says "410 ResourceExpired"; the
   `Status.reason` constant in `apimachinery` is `Expired` (`StatusReasonExpired`), with
   `Gone` used by older servers. The SDK classifies by the HTTP status `410` and accepts either
   reason in watch `ERROR` events.
6. **Namespace names** are validated as RFC 1123 labels, object names as RFC 1123 subdomains
   (the general rule; some kinds are stricter and the server enforces that).
7. **Ingress field selectors.** [K2]
   <https://kubernetes.io/docs/concepts/overview/working-with-objects/field-selectors/> (retrieved
   2026-11-05) says every resource type supports `metadata.name` and `metadata.namespace`, lists
   no Ingress-specific field, and its own `kubectl get ingress --field-selector foo.bar=baz` error
   example names only those two as known fields. Field selectors take `=`, `==` and `!=` only
   (no set-based operators). The SDK's `FieldSelector` does not know which fields a kind supports;
   the server rejects an unsupported one with an error.
8. **Ingress class.** `spec.ingressClassName` replaces the deprecated
   `kubernetes.io/ingress.class` annotation; the model decodes the field and leaves the annotation
   in `metadata.annotations` (no merging).
