# Shopify GraphQL Admin API — pinned version and sources

This package is a **bounded slice** of the Shopify GraphQL Admin API, not a complete Shopify SDK.

## Pinned version

- **`2026-10`** (`ApiVersion::PINNED`), sent in the path
  `POST https://{shop}.myshopify.com/admin/api/2026-10/graphql.json`.
- On 4 November 2026 the versioning page listed `2026-01`, `2026-04`, `2026-07` and `2026-10` as
  supported stable versions, `2026-10` as the latest stable, and `2027-01` as a release candidate
  (not used). This matches the handoff's earlier observation of stable `2026-10`.
- Shopify serves a request for an inaccessible version with the oldest accessible stable version
  ("falls forward") and reports the version used in `X-Shopify-API-Version`.
  `ShopContext::send` refuses a 2xx answer whose header differs from the requested version
  (`RequestError::VersionMismatch`) or is missing (`RequestError::VersionUnconfirmed`), before
  reading the body.
- No machine-readable schema digest was pinned: the field types and nullability below were read
  from the versioned (`2026-10`) reference pages, retrieved 4 November 2026.

## Sources (retrieved 4 November 2026)

| Topic | URL |
|---|---|
| Versioning (V3) | <https://shopify.dev/docs/api/usage/versioning> |
| Endpoint, `X-Shopify-Access-Token`, status codes, error codes, cost example (H1) | <https://shopify.dev/docs/api/admin-graphql/latest> (showing `2026-10`) |
| Cost and throttle status | <https://shopify.dev/docs/apps/build/apis/graphql-admin/rate-limits>, <https://shopify.dev/docs/api/usage/limits> |
| Connection pagination (H2) | <https://shopify.dev/docs/api/usage/pagination-graphql> |
| Bulk queries and the JSONL result (H3) | <https://shopify.dev/docs/apps/build/apis/graphql-admin/bulk-operations/queries> |
| Search syntax | <https://shopify.dev/docs/api/usage/search-syntax> |
| Objects: `Product`, `ProductVariant`, `Collection`, `Location`, `LocationAddress`, `InventoryLevel`, `InventoryQuantity`, `InventoryItem`, `MoneyV2`, `Count`, `Shop`, `BulkOperation`, `BulkOperationUserError` | `https://shopify.dev/docs/api/admin-graphql/2026-10/objects/<Name>` |
| Queries: `products`, `product`, `location`, `bulkOperation` | `https://shopify.dev/docs/api/admin-graphql/2026-10/queries/<name>` |
| Mutations: `bulkOperationRunQuery`, `bulkOperationCancel` | `https://shopify.dev/docs/api/admin-graphql/2026-10/mutations/<name>` |
| Enums: `BulkOperationStatus`, `BulkOperationErrorCode`, `BulkOperationUserErrorCode`, `CountPrecision` | `https://shopify.dev/docs/api/admin-graphql/2026-10/enums/<Name>` |
| Scalars: `Decimal`, `Money` (strings), `UnsignedInt64` (string of digits) | `https://shopify.dev/docs/api/admin-graphql/2026-10/scalars/<Name>` |
| Inventory state names (`available`, `on_hand`, ...) | <https://shopify.dev/docs/apps/build/orders-fulfillment/inventory-management-apps> |

No live shop was contacted. Every claim above is documentation-derived; nothing here is
`LIVE_PROVIDER_VERIFIED`.

## Supported operations

Eleven named GraphQL operations, each a generated endpoint of `ShopifyAdmin.shop` (one
`graphql.json` endpoint, one typed variables body and one `Answer<Op>` output per operation),
plus one native download:

| Family | Endpoint | Document (`operationName`) | Kind |
|---|---|---|---|
| Shop | `shop_info` | `ShopIdentity` — `shop { id name myshopifyDomain currencyCode }` | query |
| Catalog | `products` | `CatalogProducts` — `products(first, after, query, sortKey: ID)` with each product's first `variants(first)`; `onlineStoreUrl @include(if: $withOnlineStoreUrl)` on request (`ProductsQuery::with_online_store_url`) | query |
| Catalog | `product` | `CatalogProduct` — `product(id)` with tags, `variantsCount`, first variants and collections | query |
| Catalog | `product_variants` | `ProductVariants` — `product(id) { variants(first, after) }` (nested connection) | query |
| Catalog | `collections` | `CatalogCollections` — `collections(first, after, query, sortKey: ID)` with `productsCount` and first members | query |
| Catalog | `collection_products` | `CollectionProducts` — `collection(id) { products(first, after) }` (nested connection) | query |
| Inventory | `locations` | `Locations` — `locations(first, after, includeInactive)` | query |
| Inventory | `location_inventory` | `LocationInventory` — `location(id) { inventoryLevels(first, after) }` with `quantities(names)` and the item (`sku`, `tracked`, `unitCost`) | query |
| Bulk | `run_bulk_query` | `BulkRunQuery` — `bulkOperationRunQuery(query)` | **mutation** (creates a job) |
| Bulk | `bulk_operation` | `BulkOperationStatus` — `bulkOperation(id)` | query |
| Bulk | `cancel_bulk_operation` | `BulkOperationCancel` — `bulkOperationCancel(id)` | **mutation** |
| Bulk | `ShopContext::result_download` (native `GET`) | the signed `url` / `partialDataUrl`, on the storage host, without the access token | download |

`bulk_export::CatalogExport` builds the bulk query document (products, optionally their variants
and their metafields as nested connections) and `bulk_export::BulkRecord` decodes its JSONL lines;
a selection with metafields is read as `bulk_export::CatalogLine` (`Record(BulkRecord)` or
`Metafield(BulkMetafield)`), since `BulkRecord` refuses a metafield line.

## Deliberately unsupported

- Every other resource (orders, customers, fulfillment, media, publications, ...) and every
  mutation other than the two bulk ones; `bulkOperationRunMutation`. Metafields are read only as
  product children of the bulk catalog export (`id namespace key value type`; not `jsonValue`,
  `owner`, variant metafields or the `keys:` argument).
- `currentBulkOperation`: deprecated from `2026-01`, replaced by `bulkOperation(id:)`.
- The `bulkOperations` listing, `groupObjects: true`, and the `bulk_operations/finish` webhook
  (polling is used; the docs recommend it as the fallback anyway).
- Backward pagination (`last`/`before`) and the `edges { cursor node }` form in the
  interactive queries (`nodes` is the documented preference); the bulk document uses `edges`,
  as the bulk examples do.
- Token acquisition (OAuth, custom app installation), retries, backoff, and the
  `Shopify-GraphQL-Cost-Debug` header (its `fields` breakdown would be kept dynamic in
  `QueryCost::other`).
- The Storefront API and the REST Admin API.

## Documented discrepancies, gaps and how they are resolved

1. **`extensions.cost` has two shapes.** The response-level `extensions.cost` is an object
   (`requestedQueryCost`, `actualQueryCost`, `throttleStatus{...}`), but the documented
   `MAX_COST_EXCEEDED` error carries `errors[].extensions.cost` as a **number** (with `maxCost`).
   `QueryCost` types the former; `ErrorExtensions::other` keeps the latter dynamic.
2. **No official `THROTTLED` example.** The code is documented (`errors[].extensions.code`, HTTP
   200, "similar to 429"). The fixtures' throttled body is synthetic: no `data`, and
   `actualQueryCost: null` (the query did not run). `QueryCost::actual_query_cost` is therefore an
   `Option` (present-but-nullable); this is an assumption, not an observation.
3. **Number shapes.** The examples show `maximumAvailable` as `1000.0` and `1000`, and
   `currentlyAvailable` as an integer; all cost numbers are `f64`.
4. **`variables` are not shown** on the Admin API reference page (its examples send only
   `query`), while the pagination guide uses `$cursor` / `$numProducts` variables. The SDK sends
   the standard GraphQL-over-HTTP body `{query, operationName, variables}`.
5. **Result file `Content-Type` is not documented.** The reader uses `MediaPolicy::any()`
   deliberately (the status is still checked before reading; the body must still be UTF-8 JSON
   per line).
6. **Expired signed URL status is not documented** (the URL "expires 7 days after the operation
   completes"; status `EXPIRED` exists). Any non-2xx download is refused before reading
   (`ExportOutcome::DownloadRefused`, response kept unread); the fixture uses `403`.
7. **Non-connection objects in JSONL.** The docs say each line is a connection node and nested
   connections get their own lines with `__parentId`; that a plain nested object
   (`inventoryItem { id tracked }`) stays inline on its parent's line is inferred.
8. **Ordering.** "All nested connections appear after their parents"; adjacency is not promised.
   The export fixture interleaves (a variant of product 101 after product 103), and the workflow
   emits normalized records (each variant names its product) instead of grouping.
9. **Search syntax quoting.** The page lists `:`, `\`, `(`, `)` as characters to escape and allows
   double or single quotes, but does not say how a quote inside a quoted value is written:
   `SearchQuery` escapes `"` and `\` with `\`.
10. **Return types inferred from the reference layout:** `locations` (a `LocationConnection`),
    `collections` (a `CollectionConnection`), `product(id:)`, `location(id:)` and
    `bulkOperation(id:)` (nullable: listed without `!`).
11. **Redirects.** reqwest's default redirect policy forwards custom headers such as
    `X-Shopify-Access-Token` to the redirect target; the documented API does not redirect, so the
    SDK's documentation and the live binaries use `redirect::Policy::none()`.
12. **`UnsignedInt64`** is documented as a string of digits; a JSON number is also accepted
    (lenient, never produced by the documented schema).
13. **Bulk query limits** (at most five connections, two levels of nesting, a connection at the
    root, no top-level `node`/`nodes`): `CatalogExport` uses at most three connections and one
    level.
14. **`@include` on the Admin API** (added 5 November 2026 with `onlineStoreUrl`). `@include` and
    `@skip` are the GraphQL specification's built-in directives; the Admin API reference does not
    list them, and their support is confirmed only in Shopify's developer community
    (<https://community.shopify.dev/t/skip-and-include-directives/8255>, February 2025, not
    API-specific). `CatalogProducts` relies on `onlineStoreUrl @include(if: $withOnlineStoreUrl)`
    with `$withOnlineStoreUrl: Boolean! = false`, omitted unless requested, so callers that do not
    ask send the same variables as before and receive no new member. Not live-verified.
15. **`onlineStoreUrl`** (`URL`, nullable; `Product` page for `2026-10`, retrieved 5 November
    2026): "If `null`, then the product isn't published to the online store sales channel". A
    member that is absent therefore must not be read as `null`: `Included::NotIncluded` (not
    requested) and `Included::Null` (unpublished) stay distinct.
16. **Metafields in bulk** (`Metafield` and `Product.metafields` pages for `2026-10`, retrieved 5
    November 2026: `id: ID!`, `namespace`, `key`, `value`, `type: String!`;
    `metafields(namespace:)` filters, "If omitted, all metafields are returned"). The bulk guide
    names `metafields` only as a nested connection in its nesting-limit example and shows
    arguments only on the root connection; that `namespace:` applies to a nested connection in a
    bulk query is an assumption. A metafield line is inferred to follow the documented child-line
    rule (`__parentId`: its product).

## Evidence labels (implementation SHA in the delivery report)

| Workflow | Tier | Label | Scope |
|---|---|---|---|
| SH-A | offline loopback fixtures | `OFFLINE_VERIFIED` | two synthetic shops, schema-derived and fault-injection fixtures, `cargo test -p concord-example-shopify-app --test sh_a_catalog_inventory` |
| SH-A | live shop | `NOT_RUN` | no credentials; `cargo run -p concord-example-shopify-app --bin sh_a` prints `NOT_RUN` |
| SH-B | offline loopback fixtures | `OFFLINE_VERIFIED` | synthetic shop + storage host, `cargo test -p concord-example-shopify-app --test sh_b_bulk_export` |
| SH-B | live development shop | `NOT_RUN` | no credentials and no job-creation opt-in; `--bin sh_b` prints `NOT_RUN` |
