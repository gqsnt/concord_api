# Shopify SDK maintenance exercise (DX-01, `DATA_AND_VALIDATION.md` §6)

## Task statement

Staged SDK evolution (not a claim that Shopify changed): **evolve one named GraphQL document of
the Shopify slice (`examples/shopify-sdk`) with an optional selected field and add a related bulk
record shape**. Evidence to retain: typed response/source changes, old consumer behaviour,
handling of absent/unknown fields, parent-child parsing changes. Procedure:
`v4/extensions-real-apis/MAINTENANCE_BRIEF.md` (three commits: frozen consumer, SDK change,
updated consumer; findings recorded afterwards).

The concrete change, chosen after checking the `2026-10` reference on shopify.dev (5 November
2026): `CatalogProducts` gains the product's **`onlineStoreUrl`** (`URL`, nullable: "If `null`,
then the product isn't published to the online store sales channel"), and the bulk catalog export
gains the products' **metafields** (`id namespace key value type`, all non-null) as a third JSONL
record kind with `__parentId`. The suggested variant `barcode` was rejected: it is **deprecated**
in `2026-10` (replaced by the `barcodes` connection).

Performed by a maintainer who did not write the SDK, reading its rustdoc, `PROVIDER.md`, the
briefs, Concord's rustdoc and Shopify's documentation. `examples/shopify-sdk/FRICTION.md` was not
read before the stage-3 commit; the comparison is a separate, labelled section at the end. Same
model family as the SDK author (a Claude agent), so independence is limited to "did not write it
and did not read its author's friction notes". The session was interrupted once by a machine
restart between writing and committing stage 1 (the uncommitted work survived and was re-verified
before the commit).

## Commits and commands

| Stage | Commit | Content |
|---|---|---|
| 1 frozen consumer | `d72e4699297c8bfe523e2b30a177a946f055a945` | `src/before.rs`, `tests/before.rs`, `Cargo.toml`, `Cargo.lock` |
| 2 SDK change | `ee7145adf4d95751e66bf33ff7335e66dc7e7612` | `examples/shopify-sdk/{src,tests,PROVIDER.md}` only (app untouched) |
| 3 updated consumer | `183959fa34cf5f8bed7db000316622b4704495f6` | `src/after.rs`, `tests/after.rs`, `Cargo.toml` (+`serde`), `Cargo.lock` |
| findings | the commit adding this file | `MAINTENANCE.md` |

Executable check: `cargo test --locked -p concord-check-maintenance-shopify` — stage 1: 3 tests
(`tests/before.rs`); stage 2: the same 3 unchanged tests against the changed SDK; stage 3: 3 + 4
(`tests/after.rs`). SDK and app at stages 2 and 3:
`cargo test --locked -p concord-example-shopify-sdk -p concord-example-shopify-app`. Also
`cargo clippy --locked --all-targets -- -D warnings` and `cargo fmt --check` on the three packages,
`RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p concord-example-shopify-sdk`. Rust 1.99.1,
`CARGO_BUILD_JOBS=6`, Linux x86_64, loopback only. Results: see the end.

## Stage 1 — frozen initial consumer

`src/before.rs` is a downstream **product-feed job** using only documented public exports:

- `list_active_products(shop, page_size, budget)`: `CatalogProducts` (`status:active`, one variant
  per product) through `concord_pagination::sequential` and the SDK's `connection_page`; each
  `ProductNode` becomes a `FeedProduct` (`From<ProductNode>`); a page with GraphQL errors fails
  the listing.
- `feed_export()` / `submit_feed_export(shop)`: the bulk selection
  `CatalogExport::products().filter(status:active).with_variants(true)` and its submission.
- `stream_feed_export(shop, id, sink)`: one status check, then the native signed download opened
  with `bulk_export::result_reader(..).open_for::<BulkRecord>(..)`; an **exhaustive `match`** on
  `BulkRecord::{Product, Variant}` maps each line to a `FeedLine` for the sink (the natural code,
  and the same shape as the SDK's own app).

`tests/before.rs` (3 tests) runs it against `concord-example-testserver`: two shops with distinct
synthetic tokens (a leaked token gets a `401`), a storage host, and **exact request bytes** for
every GraphQL request (`{"query":<O::DOCUMENT>,"operationName":..,"variables":{..}}` with the
variables written out literally; the document referenced through the SDK's public
`Operation::DOCUMENT`, because it is SDK-owned), the bulk document written out literally (the
consumer pins the selection it chose), and the download (verbatim signed path, storage host, no
`X-Shopify-Access-Token`).

Stage-1 probes against the **old** SDK (uncommitted, output kept as evidence here):

| Probe | Observed |
|---|---|
| Old `ProductsData` meets an answer that carries `onlineStoreUrl` (a URL, and `null`) | decodes (`Ok`), the extra member is ignored: the SDK never uses `deny_unknown_fields` |
| Old `BulkRecord` reader meets a metafield child line (`RecordErrors::Stop` and `Yield`) | `ReadErrorKind::Decode` at that line, `Position { records: 1, bytes: 347 }`; Display `a record does not decode: a record does not match the expected type (line 2, column 0) (after 1 records, 347 bytes)`; `source_unredacted()` = `a line of an object type the selection does not contain`; `Yield` then continues with the next product |
| Frozen consumer's first `CatalogProducts` body | 575 bytes; variables `{"first":2,"query":"status:active","variantsFirst":1}` |

## Stage 2 — the SDK change

### Approaches tried before the committed one (uncommitted probes, reverted)

**Probe S — always select the field, the SDK's documented convention.** `ops/mod.rs` says:
"Nullable fields that are selected are read with `nullable`: `null` is `None`, an absent field is
a decode error." Adding `onlineStoreUrl` to the document and
`#[serde(deserialize_with = "nullable")] online_store_url: Option<String>` to `ProductNode`
compiled everywhere and broke **at runtime** every recorded answer to the old document:

| Package | Result under probe S |
|---|---|
| frozen consumer | 1 of 3 tests fails: `Page(Request(Concord(Error { kind: Decode, endpoint: Some("ShopifyAdmin.shop.products"), message: POST ShopifyAdmin.shop.products: response not decoded: the JSON does not match the expected type (line 1, column 545) })))` |
| SDK tests | 2 of 8 decoding tests fail (`nullable_fields_accept_null_and_refuse_absence`, `the_adapter_selects_the_dependent_cursor_and_refuses_inconsistent_page_info`) |
| SDK app | 5 of 6 SH-A tests fail (traversals end `Failed` at the first `CatalogProducts` page); 4 fixture files lack the member, and their `MANIFEST.json` sha256 entries and `generate.py` would have to change too |

The message withholds the cause: Concord redacts serde's text. Reaching it took four layers —
`FeedError::Page` → `PageError::Request` → `RequestError::Concord` → `Error::codec_error()` →
`CodecError::source_unredacted()` = `missing field `onlineStoreUrl` at line 1 column 305`. The
convention is right for detecting selection/model drift, but its maintenance consequence (an
always-selected additive field is a runtime break for every stored answer, invisible to the
compiler) is not documented. Reading an absent member as `None` instead (`#[serde(default)]`)
would have been worse for this field: an old answer would silently read as "not published".

**Probe E — a third `BulkRecord` variant.** Adding `BulkRecord::Metafield(..)` broke exhaustive
matches downstream (the SDK itself compiled):

```text
error[E0004]: non-exhaustive patterns: `BulkRecord::Metafield(_)` not covered
   --> checks/maintenance-shopify/src/before.rs:359:26
    |
359 |         let line = match record? {
    |                          ^^^^^^^ pattern `BulkRecord::Metafield(_)` not covered
```

and the same error twice in `examples/shopify-app/src/bulk_export.rs` (lines 199 and 659).
**Probe NE** — retrofitting only `#[non_exhaustive]` on `BulkRecord`, without a new variant — is
breaking too: `error[E0004]: non-exhaustive patterns: `_` not covered` at `before.rs:359`
("`BulkRecord` is marked as non-exhaustive, so a wildcard `_` is necessary"). Both errors point at
the right place and suggest the right fix. So the closed enum cannot grow compatibly at all.

### What was committed

1. **`CatalogProducts` evolved with an opt-in field** (`ops/catalog.rs`):
   `onlineStoreUrl @include(if: $withOnlineStoreUrl)` with `$withOnlineStoreUrl: Boolean! = false`;
   `ProductsQuery` gains `with_online_store_url: Var<bool>` (absent unless
   `ProductsQuery::with_online_store_url()` is called; `Var::Absent` is omitted from `variables`),
   and `ProductNode` gains `#[serde(default)] online_store_url: Included<String>`.
2. **`graphql::Included<T>`** (new, 23 code lines): `NotIncluded` (absent: not requested) /
   `Null` / `Value(T)`; the one documented exception to "absent is a decode error".
3. **Bulk metafields** (`bulk_export.rs`): `CatalogExport::with_metafields(Option<&str>)` appends
   ` metafields(namespace: "..") { edges { node { id namespace key value type } } }` (namespace as a
   GraphQL string literal, like the filter); `BulkMetafield { id, product_id (from __parentId),
   namespace, key, value, value_type }`; four optional fields on the private `Line` union; and a
   new **`#[non_exhaustive]` `CatalogLine { Record(BulkRecord), Metafield(BulkMetafield) }`** whose
   decoder tries the `Metafield` gid type and otherwise delegates to `BulkRecord`'s own
   `TryFrom<Line>`. `BulkRecord` is **unchanged** and still refuses a metafield line.
4. `scalars.rs`: `Metafield => MetafieldId;` in the existing `kinds!` macro (1 line).
5. Re-exports (`Included`, `CatalogLine`), module docs, `PROVIDER.md` (supported/unsupported,
   discrepancies 13–16: `@include` support, `onlineStoreUrl` null semantics, metafields in bulk).

Why `@include` rather than always selecting: callers that do not opt in send byte-identical
variables, receive no new member, pay no new selection, and their stored answers stay exactly what
Shopify returns for their request — so the frozen consumer's fixtures remain *faithful*, not
merely tolerated. The cost is an assumption: `@include`/`@skip` are GraphQL built-ins, but the
Admin API reference does not list them; support is confirmed only in Shopify's developer community
(February 2025). Not live-verified (`PROVIDER.md` discrepancy 14). Equally an assumption: that a
nested connection's `namespace:` argument applies in a bulk query (discrepancy 16).

### Diff of stage 2 (`git diff --stat d72e4699 ee7145ad`)

```text
 examples/shopify-sdk/PROVIDER.md        |  33 +++++++--
 examples/shopify-sdk/src/bulk_export.rs | 118 +++++++++++++++++++++++++++++++-
 examples/shopify-sdk/src/graphql.rs     |  40 +++++++++++
 examples/shopify-sdk/src/lib.rs         |  10 +--
 examples/shopify-sdk/src/ops/catalog.rs |  30 ++++++--
 examples/shopify-sdk/src/ops/mod.rs     |   2 +
 examples/shopify-sdk/src/scalars.rs     |   2 +
 examples/shopify-sdk/tests/decoding.rs  | 100 ++++++++++++++++++++++++++-
 examples/shopify-sdk/tests/wire.rs      |  72 ++++++++++++++++++-
 9 files changed, 388 insertions(+), 19 deletions(-)
```

| Kind | Added lines (code / doc comments) |
|---|---|
| `concord::api!` declaration (`api.rs`) | **0** — `POST products(body variables: ProductsQuery) -> Answer<ProductsQuery>` already derives its output from the operation |
| GraphQL document text (`DOCUMENT` const) | 4 added, 2 removed |
| Variables / response model (`ProductsQuery` field + builder, `ProductNode` field, imports) | 11 code |
| `Included<T>` | 23 code, 13 doc |
| Bulk selection + record + `CatalogLine` decoder (`bulk_export.rs`) | 77 code, 31 doc |
| gid kind, re-exports | 4 code |
| SDK tests (new tests only; no existing test edited) | `wire.rs` +71 (`online_store_url_is_selected_only_when_requested`), `decoding.rs` +99 (`metafield_lines_are_children_of_their_product_and_only_catalog_line_reads_them`) |
| App | **0** (`examples/shopify-app` compiles and passes unchanged) |

### Old call sites and old consumer behaviour

- **Compiled unchanged**: the frozen consumer, the SDK's app and the SDK's existing tests. No
  compile error from the committed change.
- **Wire bytes of the frozen consumer** (the document is SDK-owned, shared by every caller):
  `CatalogProducts` body 575 → **663 bytes**: `query` text +88 bytes (`, $withOnlineStoreUrl:
  Boolean! = false` and ` onlineStoreUrl @include(if: $withOnlineStoreUrl)`); `operationName` and
  `variables` byte-identical. The frozen bulk document is byte-identical (pinned literally in
  `tests/before.rs`, and by the SDK's own `bulk_documents_escape_the_filter_as_a_graphql_string`).
  The updated consumer's variables add `,"withOnlineStoreUrl":true` (689 bytes).
- **Old consumer meets an answer carrying the new member**: at stage 1 the old model ignored it
  (probe above); at the final commit `tests/after.rs::the_link_member_across_old_and_new_answers`
  runs the frozen `list_active_products` against answers with `onlineStoreUrl` (URL and `null`)
  and without it: identical `FeedProduct` rows, and its request still omits the flag.
- **New model meets an older answer without the member**: `Included::NotIncluded` (decodes). For a
  caller that requested it this is a mismatch the decoder cannot detect (the response does not
  echo the variables); the updated consumer refuses it explicitly
  (`UpdatedFeedError::LinkNotIncluded { product }`, tested), never treating it as "unpublished".
- **Old bulk reader meets the new file**: `tests/after.rs::the_frozen_reader_refuses_a_metafield_line_at_its_position`
  — the frozen `stream_feed_export` attached to an attribute export delivers the product and offer
  before the first metafield line, then fails with `FeedError::Read` (`Decode`, `records: 2`,
  `source_unredacted()` = `a line of an object type the selection does not contain`); no panic, no
  silent skip, no signed URL or record content in `Display`/`Debug`.
- **Latent hazard, not hit**: `ProductsQuery`'s fields are `pub` without `#[non_exhaustive]`; a
  caller that built it with a struct literal breaks (probe):
  `error[E0063]: missing field `with_online_store_url` in initializer of `ProductsQuery``. The
  frozen consumer used `ProductsQuery::new` (the documented path), so it was unaffected. Adding a
  `pub` field to `ProductNode` has the same hazard for literals / exhaustive destructuring.

## Stage 3 — the updated consumer

`src/after.rs` (339 lines: 250 code, 67 doc) reuses the frozen types (`FeedProduct`,
`FeedLine`, `FeedError`, `StreamReport`, `feed_export`):

- `list_published_products`: `ProductsQuery::new(..).with_online_store_url()`; per node
  `match std::mem::take(&mut node.online_store_url)` → `Value(link)` listed as
  `ListedProduct { product: FeedProduct::from(node), link }`, `Null` set aside, `NotIncluded`
  refused. (`Included: Default` is what lets the frozen `From<ProductNode>` be reused unchanged.)
- `attribute_export() = feed_export().with_metafields(Some("mm-google-shopping"))`;
  `stream_attribute_export` opens the result as `CatalogLine`; `CatalogLine::Record(record)` goes
  through the frozen mapping to `FeedLine`, `CatalogLine::Metafield(m)` becomes an attribute of
  `m.product_id`; the wildcard arm the non-exhaustive enum requires counts unknown kinds.

`tests/after.rs` (4 tests): exact wire bytes with the flag and the attribute document (written
out literally); products, offers and metafields interleaved across two parents
(`P21, V211<21, M7001<21, P22, M7002<22, M7003<21, V221<22`) with each attribute attributed by
`__parentId`; and the compatibility cases above.

## Findings

### Parent-child parsing changes

The SDK's design made the bulk change cheap and safe: every line is read into one private union
struct and classified by the **gid type** of its `id`; a child's parent comes only from
`__parentId`. A new child kind = optional fields on the union + one `TryFrom` branch that copies
the variant branch's parent check (`__parentId` must parse as a `ProductId`). Records stay
normalized per line, so interleaving needs no buffering (no hidden collection; one record per
`next()`). What does not compose is the **public** side: the decoded type is a closed enum, so
the new kind needed a second, wrapping enum. The selection (`CatalogExport` runtime flags) and
the decode type (`open_for::<BulkRecord>` vs `::<CatalogLine>`) are separate knobs coupled only by
documentation: changing one without the other compiles and fails at the first metafield line
(the frozen-reader test is exactly that situation).

### Absent / unknown handling (summary)

| Situation | Behaviour |
|---|---|
| unknown member in an answer (old model, new member) | ignored (serde default) |
| selected non-null member absent | decode error (unchanged) |
| selected nullable member absent (`nullable`) | decode error, cause withheld in `Display` (probe S) |
| `@include` member absent (`Included`) | `NotIncluded`; the caller decides (the updated consumer refuses it when it asked) |
| JSONL line of a type the record type does not know | per-line `Decode` error with position; `Yield` can skip it |
| `CatalogLine` kind added by a later SDK | compiles (wildcard arm), counted as unknown by this consumer |

### Concepts the caller had to learn

`Included<T>` (three states, and why absent is not null); the opt-in builder
`ProductsQuery::with_online_store_url()`; `CatalogExport::with_metafields(namespace)`; that the
record type must follow the selection (`CatalogLine` instead of `BulkRecord`), and that
`CatalogLine` is non-exhaustive; `BulkMetafield::product_id` comes from `__parentId` and
`value_type` is text. No new Concord concept: `connection_page`, `sequential`,
`JsonLinesReader::open_for`, `RecordErrors`, `ReadError::position` were already in use.

### Documentation read, and whether it answered

| Source | Answered? |
|---|---|
| `MAINTENANCE_BRIEF.md`, `SDK_BRIEF.md`, `DATA_AND_VALIDATION.md` §6, §12 | procedure, yes |
| `examples/shopify-sdk/PROVIDER.md` | what is selected and why, JSONL rules and inferred gaps: yes. There is no SDK README; `PROVIDER.md` + `lib.rs` rustdoc served as one |
| SDK rustdoc: `lib.rs`, `api.rs`, `ops/mod.rs`, `ops/catalog.rs`, `graphql.rs`, `bulk_export.rs`, `scalars.rs`, `connection.rs`, `context.rs` | yes for structure. `ops/mod.rs` predicted probe S ("absent is a decode error"); `variant_fields!` showed that a variant field would have evolved three documents at once (one reason to pick a product field); `kinds!` made the gid kind one line. Not answered: how to evolve a document or a record enum compatibly |
| `examples/shopify-app/README.md`, `src/bulk_export.rs`, `tests/support/mod.rs` | the exhaustive `match` on `BulkRecord` (predicted probe E) and the test pattern |
| `examples/testserver/src/lib.rs` | yes (`Script`, `Reply::text`, `Recorded`) |
| `concord-pagination` `sequential.rs`, `types.rs`; `concord-extras` `jsonl/mod.rs`, `jsonl/lines.rs`, `stream/error.rs`; `concord-core` `error.rs` | yes; `error.rs` was needed only to find `codec_error()` / `source_unredacted()` for the withheld decode cause |
| shopify.dev `2026-10` `Product`, `ProductVariant`, `Metafield`, bulk-operations queries guide (5 Nov 2026) | field types and nullability yes (and `barcode` deprecated); `@include` support and nested arguments in bulk: **not** in Shopify's reference — community thread only / assumption |

### Compiler errors met

| Error | Where | Pointed at the right cause? |
|---|---|---|
| E0004 `BulkRecord::Metafield(_)` not covered (×3) | probe E: frozen consumer, SDK app | yes, with the fix |
| E0004 `_` not covered | probe NE | yes, explains `#[non_exhaustive]` |
| E0063 missing field `with_online_store_url` | probe: struct literal | yes |
| none (runtime decode errors instead) | probe S | the compiler cannot see it; the runtime message withholds the field name |
| E0618 expected function, found `&Gid<Product>` | stage 3, my test: a binding named `product` shadowed the helper fn `product` | yes ("shadowed by the local binding"); not SDK-related |
| clippy `large_enum_variant` | stage 3, `enum Opened<T> { .., Records(JsonLinesRecords<T>) }` (304 bytes) | yes; fixed with `Box` |

Committed code compiled first time at every stage otherwise.

### Casts, annotations, adapters, native escapes

- `open_result::<CatalogLine>` turbofish; the generic helper needs `T: serde::de::DeserializeOwned`
  (mirroring `open_for`'s bound), which added a direct `serde` dependency to the consumer: Concord
  does not re-export serde.
- `std::mem::take` on `Included` to keep `FeedProduct::from(node)` reusable.
- `Box<JsonLinesRecords<T>>` for clippy.
- No new native escape: the download is the SDK's existing `ShopContext::result_download`.

### Duplicated code (cause and boundary)

About 110 of `after.rs`'s 339 lines repeat frozen helpers with one line or token changed:
`linked_page` (= `products_page` + `.with_online_store_url()`), `submit_attribute_export` (=
`submit_feed_export` with another selection), `open_result` (the status/download half of
`stream_feed_export`, record type as a parameter) and `feed_line` (the inline `BulkRecord` →
`FeedLine` mapping). The cause is the frozen consumer's own structure — private helpers with the
query flag, the selection and the record type hard-coded — not an SDK or Concord requirement; a
consumer evolved in place would have changed those lines instead. The SDK side contributed one
thing: selection and record type must change together and nothing ties them.

### Metadata not preserved

Nothing the change added is lost: `Included` keeps not-requested vs `null`; metafield `type` is
kept as text. Deliberately not selected: `jsonValue` (dynamic JSON), `owner`, `ownerType`,
timestamps, `compareDigest`, variant metafields.

### Counter-evidence (what was easy)

- **Zero `concord::api!` lines** changed: an operation type carries its document and its `Data`,
  and the endpoint's output `Answer<ProductsQuery>` follows it.
- Frozen consumer, SDK app and existing SDK tests unchanged and green; the non-opted-in wire
  variables and the default bulk documents are byte-identical.
- `Var::Absent` (omitted from `variables`) gave the opt-in flag its compatibility for free.
- A gid kind is one line; a JSONL record kind is a union field set + one `TryFrom` branch;
  switching the reader's record type is one turbofish — framing, limits, position and error
  kinds unchanged, and the old reader fails typed and positioned rather than silently.
- The pagination path (`connection_page`, `sequential`) needed no change at all.

### Timings (secondary)

Wall clock of an agent, not a human: stage 2 commit 5 minutes after stage 1's, stage 3 four
minutes later; most design time (reading, schema checks, choosing between the approaches) happened
before the restart and is not measurable. Not a quality signal.

## Proposals (not implemented; no `crates/` code was edited)

1. **SDK: tie the record type to the selection.** Before (two knobs):

   ```rust
   let export = feed_export().with_metafields(Some(FEED_NAMESPACE));
   let records = result_reader(max, errors).open_for::<CatalogLine>(response, BULK_RESULT_OPERATION)?;
   ```

   After (the selection knows its line type):

   ```rust
   let export = feed_export().with_metafields(Some(FEED_NAMESPACE)); // CatalogExport<CatalogLine>
   let records = export.open(result_reader(max, errors), response)?;  // JsonLinesRecords<CatalogLine>
   ```

   `CatalogExport<R = BulkRecord>` with `with_metafields(self) -> CatalogExport<CatalogLine>` and
   `fn open(&self, reader: JsonLinesReader, response) -> Result<JsonLinesRecords<R>, OpenError>`
   wrapping `open_for::<R>`; SDK-level, no library change.
2. **SDK: make growth explicit from the start.** `#[non_exhaustive]` on public response enums
   that mirror a growing selection (retrofitting it is itself breaking, probe NE), and
   `#[non_exhaustive]` (or private fields) on variables structs that already have `new` +
   builders (E0063 hazard).
3. **SDK docs (`ops/mod.rs`): state the evolution rule.** An always-selected field is a runtime
   break for every stored answer (probe S); an additive optional field should use
   `@include` + `Included`, or the change must regenerate fixtures.
4. **Concord (low priority, cost not shown to pay off):** the withheld decode cause cost four
   layers of unwrapping. serde reports a missing field through `de::Error::missing_field(&'static
   str)` — a schema name, never body content — so a JSON decoder wrapper could keep it as a
   structured detail. Before: `e.codec_error().and_then(|c| c.source_unredacted())` →
   `missing field `onlineStoreUrl` at line 1 column 305`. After (sketch):
   `e.codec_error().and_then(|c| c.missing_field()) == Some("onlineStoreUrl")`, and `Display`
   `the JSON does not match the expected type: missing field onlineStoreUrl (line 1, column 305)`.

## Results

| Stage | Command | Result |
|---|---|---|
| 1 (`d72e4699`) | `cargo test --locked -p concord-check-maintenance-shopify` | `before`: 3 passed |
| 2 (`ee7145ad`) | same | `before`: 3 passed (unchanged tests, changed SDK) |
| 2 | `cargo test --locked -p concord-example-shopify-sdk -p concord-example-shopify-app` | SDK: decoding 9, wire 8, doctest 1 passed; app: manifest 2, sh_a 6, sh_b 12 passed |
| 3 (`183959fa`) | `cargo test --locked -p concord-check-maintenance-shopify` | `after`: 4 passed, `before`: 3 passed |
| 3 | SDK + app as above | same counts, all passed |
| 1–3 | `cargo clippy --locked --all-targets -- -D warnings`, `cargo fmt --check` (check crate, SDK, app); SDK rustdoc `-D warnings` | clean |

## Fixtures and limits

All answers and JSONL bodies are inline, `schema_derived_synthetic` (types and nullability from
the `2026-10` reference pages retrieved 5 November 2026; synthetic ids, tokens, signatures, shop
handles). No live shop was contacted: `@include` support on the Admin API, the nested
`namespace:` argument in bulk queries and the exact metafield JSONL line are documentation-derived
assumptions, not observations. Probe outputs were captured from uncommitted test files and are
quoted above verbatim; probes are not part of the executable check.

## After stage 3: comparison with the SDK author's `FRICTION.md`

Read only after the stage-3 commit (`183959fa`); nothing above was changed after reading it.

| `FRICTION.md` entry | This exercise |
|---|---|
| F2 — `Answer<O> = GraphqlResponse<<O as Operation>::Data>` instead of restating the output | **Confirmed with maintenance evidence:** evolving `CatalogProducts` changed zero declaration lines because the output follows the operation. |
| F1 — variables as a typed body with `Var<T>` | **Confirmed:** `Var::Absent` made the new `withOnlineStoreUrl` flag invisible on the wire for callers that do not opt in. |
| F8 — heterogeneous JSONL lines via a union struct + `TryFrom` by gid type | **Confirmed for the private side** (a new kind was cheap), **extended**: the *public* `BulkRecord` enum is closed, so a new kind cannot be added compatibly (probes E and NE, E0004); `FRICTION.md` does not consider growth. |
| F4 — the native download's `reqwest::Error` prints the signed URL unless `without_url()` is called | **Persists for downstream callers:** both consumers had to repeat `.map_err(\|e\| FeedError::Download(e.without_url()))`; I knew to do it only because the SDK's app does it. The proposal (`open_for_result`) would have removed it from both. |
| F5, F6, F7 (traversal loops, nested checkpoints, terminal vs isolable read errors) | Not exercised beyond agreement: the feed consumer uses one chain and `RecordErrors::Stop`. |
| — | **Not in `FRICTION.md`** (authoring log, not an evolution log): the maintenance cost of the strict absent rule (probe S), `Operation::DOCUMENT` being one constant per type (an optional selection needs `@include` or a new type), the selection/record-type coupling, the `pub`-field literal hazard (E0063), and the withheld decode cause. |

No entry contradicts an observation here. Shared authorship caveat: the SDK author and this
maintainer are the same model family, so agreement on F1/F2/F8 is weaker evidence than the
compiler and test outcomes recorded above.
