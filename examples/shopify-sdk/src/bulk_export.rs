//! A typed catalog export selection for a bulk query, the JSONL result line it produces, and the
//! configured result reader.
//!
//! # The result file (pinned `2026-10`, see `PROVIDER.md`)
//!
//! A completed bulk query's `url` points to a signed, expiring object-storage URL on another
//! host (`storage.googleapis.com` in Shopify's example). The file is JSON Lines: one object per
//! line, each a node of a connection; nested connections are written as **their own lines**,
//! after their parents, with `__parentId` naming the parent; non-connection objects
//! (`inventoryItem { id tracked }`) stay inline. A query that produced no results has `url:
//! null` (no file). A failed operation may expose `partialDataUrl` with the same format.
//!
//! [`BulkRecord`] decodes one line of the [`CatalogExport`] selection into a typed product or
//! variant, the variant keeping its parent id: records are normalized per line, so a consumer
//! never needs to hold a parent group to relate them.
//!
//! A selection [`with_metafields`](CatalogExport::with_metafields) also writes each product's
//! metafields as child lines (`__parentId`: the product). [`BulkRecord`] does not know that line
//! type and refuses it (a `Decode` error of that line, as for any unselected type); read such a
//! file as [`CatalogLine`], which is [`CatalogLine::Record`] for every product or variant line and
//! [`CatalogLine::Metafield`] for a metafield line.

use std::fmt;

use concord::jsonl::{
    JsonLinesOptions, JsonLinesReader, JsonLinesRecords, MediaPolicy, RecordErrors,
};
use concord::reqwest;
use concord::stream::OpenError;
use serde::Deserialize;
use serde::de::{DeserializeOwned, Deserializer};

use crate::graphql::string_literal;
use crate::ops::bulk::RunBulkQuery;
use crate::ops::catalog::{ProductStatus, VariantInventoryItem};
use crate::scalars::{DateTime, Decimal, MetafieldId, ProductId, ProductVariantId};
use crate::search::SearchQuery;

/// The operation label of result-download errors (`ReadError::operation`, `OpenError::operation`),
/// attached by [`open_result`].
pub const BULK_RESULT_OPERATION: &str = "shopify: bulk result";

/// The default per-line limit of [`result_reader`]: 1 MiB, far above the selection's lines
/// (a few hundred bytes) and far below the reader's 16 MiB default.
pub const DEFAULT_MAX_LINE_BYTES: usize = 1024 * 1024;

/// What a catalog export selects: products (optionally filtered), optionally with their variants
/// and their metafields as child lines.
#[derive(Clone, Debug, Default)]
pub struct CatalogExport {
    filter: SearchQuery,
    variants: bool,
    /// `None`: no metafields; `Some(None)`: every namespace; `Some(Some(ns))`: one namespace.
    metafields: Option<Option<String>>,
}

impl CatalogExport {
    /// Every product, without variants.
    pub fn products() -> Self {
        Self::default()
    }

    /// Only products matching `filter` (Shopify search syntax).
    #[must_use]
    pub fn filter(mut self, filter: SearchQuery) -> Self {
        self.filter = filter;
        self
    }

    /// With each product's variants (a nested connection: child lines with `__parentId`).
    #[must_use]
    pub fn with_variants(mut self, variants: bool) -> Self {
        self.variants = variants;
        self
    }

    /// Whether variants are selected.
    pub fn includes_variants(&self) -> bool {
        self.variants
    }

    /// With each product's metafields, of one `namespace` or (`None`) of every namespace: a
    /// nested connection, so child lines with `__parentId`. Read the result as [`CatalogLine`].
    ///
    /// The namespace is written into the document as a GraphQL string literal, like the filter.
    /// That a nested connection's `namespace:` argument applies in a bulk query is an assumption
    /// (the bulk guide shows arguments only on the root connection, see `PROVIDER.md`).
    #[must_use]
    pub fn with_metafields(mut self, namespace: Option<&str>) -> Self {
        self.metafields = Some(namespace.map(str::to_owned));
        self
    }

    /// Whether metafields are selected.
    pub fn includes_metafields(&self) -> bool {
        self.metafields.is_some()
    }

    /// The bulk query document. A bulk query takes no variables, so the filter is written into
    /// the document as a GraphQL string literal ([`string_literal`]); the document itself then
    /// travels as the JSON-encoded `$query` variable of [`RunBulkQuery`].
    pub fn document(&self) -> String {
        let arguments = if self.filter.is_empty() {
            String::new()
        } else {
            format!("(query: {})", string_literal(self.filter.as_str()))
        };
        let variants = if self.variants {
            " variants { edges { node { id title sku price inventoryQuantity \
             inventoryItem { id tracked } } } }"
        } else {
            ""
        };
        let metafields = match &self.metafields {
            None => String::new(),
            Some(namespace) => {
                let arguments = namespace.as_deref().map_or(String::new(), |namespace| {
                    format!("(namespace: {})", string_literal(namespace))
                });
                format!(
                    " metafields{arguments} {{ edges {{ node {{ id namespace key value type }} }} }}"
                )
            }
        };
        format!(
            "{{ products{arguments} {{ edges {{ node {{ id title handle status vendor updatedAt{variants}{metafields} }} }} }} }}"
        )
    }

    /// The submission mutation for this selection.
    pub fn run(&self) -> RunBulkQuery {
        RunBulkQuery {
            query: self.document(),
        }
    }
}

/// A JSON Lines reader for bulk results: strict JSON Lines framing, a per-line limit, and the
/// chosen malformed-line policy.
///
/// Media policy: [`MediaPolicy::any`], deliberately. Shopify documents the file as JSONL but not
/// the `Content-Type` its object storage serves, and the signed download is not an API response;
/// the body is still validated as UTF-8 JSON line by line, so an HTML or XML body fails as a
/// `Format` error at its first line. The status (2xx) is still checked when it is opened.
pub fn result_reader(max_line_bytes: usize, record_errors: RecordErrors) -> JsonLinesReader {
    JsonLinesReader::new(
        JsonLinesOptions::json_lines()
            .max_record_bytes(max_line_bytes)
            .record_errors(record_errors)
            .media(MediaPolicy::any()),
    )
}

/// Opens the response of a signed bulk-result download
/// ([`ShopContext::result_download`](crate::ShopContext::result_download), executed natively)
/// with `reader`, its errors naming [`BULK_RESULT_OPERATION`]: the label is the SDK's, not each
/// caller's. The reader checks the status before polling the body: a non-2xx (a refused or
/// expired signature is a `403`) is an [`OpenError`] of kind `Status` that keeps the response
/// unread ([`OpenError::into_response`]).
///
/// No declaration produced this response, so its identity is a static label. It is given to
/// the reader rather than wrapped as `concord::Incoming::new(response, Some(label))`: that
/// would make the refused status a `concord::Error` before the reader sees the response, and
/// split the download's one refusal (`OpenError`, the type existing consumers handle) into two
/// error types, the reader's own open error then being unreachable under [`MediaPolicy::any`].
///
/// # Errors
///
/// An [`OpenError`] of kind `Status` for a non-2xx response.
pub fn open_result<T: DeserializeOwned>(
    reader: &JsonLinesReader,
    response: reqwest::Response,
) -> Result<JsonLinesRecords<T>, OpenError> {
    reader.open_for(response, BULK_RESULT_OPERATION)
}

/// One line of a [`CatalogExport`] result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BulkRecord {
    /// A product line.
    Product(BulkProduct),
    /// A variant line, a child of its product.
    Variant(BulkVariant),
}

/// A product line: `{"id":"gid://shopify/Product/..", "title":.., ..}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkProduct {
    /// `ID!`
    pub id: ProductId,
    /// `String!`
    pub title: String,
    /// `String!`
    pub handle: String,
    /// `ProductStatus!`
    pub status: ProductStatus,
    /// `String!`
    pub vendor: String,
    /// `DateTime!`
    pub updated_at: DateTime,
}

/// A variant line, with its parent product from `__parentId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkVariant {
    /// `ID!`
    pub id: ProductVariantId,
    /// The parent product (`__parentId`).
    pub product_id: ProductId,
    /// `String!`
    pub title: String,
    /// `String` (nullable).
    pub sku: Option<String>,
    /// `Money!`
    pub price: Decimal,
    /// `Int` (nullable).
    pub inventory_quantity: Option<i64>,
    /// `InventoryItem!` (inline object).
    pub inventory_item: VariantInventoryItem,
}

/// A metafield line, with its owner from `__parentId` (the product: metafields are selected on
/// products only).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BulkMetafield {
    /// `ID!`
    pub id: MetafieldId,
    /// The product it belongs to (`__parentId`).
    pub product_id: ProductId,
    /// `String!`
    pub namespace: String,
    /// `String!`
    pub key: String,
    /// `String!`: the value as text, whatever its type (a JSON or list type is JSON text).
    pub value: String,
    /// `String!`: the metafield type name (`single_line_text_field`, `list.product_reference`,
    /// ...), kept as text: the set grows independently of this SDK.
    pub value_type: String,
}

/// One line of a [`CatalogExport`] result whose selection may include metafields
/// ([`CatalogExport::with_metafields`]).
///
/// Non-exhaustive: a selection that grows another child connection adds a variant here, so a
/// `match` needs a wildcard arm.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CatalogLine {
    /// A product or variant line, exactly as [`BulkRecord`] reads it.
    Record(BulkRecord),
    /// A metafield line, a child of its product.
    Metafield(BulkMetafield),
}

/// The union of the selection's fields: every line is read into it, then checked against the
/// shape of its object type (the type comes from the global id).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    id: String,
    #[serde(rename = "__parentId", default)]
    parent_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    handle: Option<String>,
    #[serde(default)]
    status: Option<ProductStatus>,
    #[serde(default)]
    vendor: Option<String>,
    #[serde(default)]
    updated_at: Option<DateTime>,
    #[serde(default, deserialize_with = "present")]
    sku: Option<Option<String>>,
    #[serde(default)]
    price: Option<Decimal>,
    #[serde(default, deserialize_with = "present")]
    inventory_quantity: Option<Option<i64>>,
    #[serde(default)]
    inventory_item: Option<VariantInventoryItem>,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(rename = "type", default)]
    value_type: Option<String>,
}

/// `Some(None)` for `null`, `Some(Some(v))` for a value; an absent field stays `None` (default).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Why a line is not a record of the selection (a static reason; the line is not quoted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineError(&'static str);

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl TryFrom<Line> for BulkRecord {
    type Error = LineError;

    fn try_from(line: Line) -> Result<Self, LineError> {
        let missing = |what| LineError(what);
        if let Ok(id) = ProductId::parse(&line.id) {
            if line.parent_id.is_some() {
                return Err(LineError("a product line has a __parentId"));
            }
            return Ok(Self::Product(BulkProduct {
                id,
                title: line.title.ok_or(missing("product line without title"))?,
                handle: line.handle.ok_or(missing("product line without handle"))?,
                status: line.status.ok_or(missing("product line without status"))?,
                vendor: line.vendor.ok_or(missing("product line without vendor"))?,
                updated_at: line
                    .updated_at
                    .ok_or(missing("product line without updatedAt"))?,
            }));
        }
        if let Ok(id) = ProductVariantId::parse(&line.id) {
            let parent = line
                .parent_id
                .ok_or(missing("variant line without __parentId"))?;
            let product_id = ProductId::parse(&parent)
                .map_err(|_| LineError("variant line whose __parentId is not a product"))?;
            return Ok(Self::Variant(BulkVariant {
                id,
                product_id,
                title: line.title.ok_or(missing("variant line without title"))?,
                sku: line.sku.ok_or(missing("variant line without sku"))?,
                price: line.price.ok_or(missing("variant line without price"))?,
                inventory_quantity: line
                    .inventory_quantity
                    .ok_or(missing("variant line without inventoryQuantity"))?,
                inventory_item: line
                    .inventory_item
                    .ok_or(missing("variant line without inventoryItem"))?,
            }));
        }
        Err(LineError(
            "a line of an object type the selection does not contain",
        ))
    }
}

impl<'de> Deserialize<'de> for BulkRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let line = Line::deserialize(deserializer)?;
        Self::try_from(line).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<Line> for CatalogLine {
    type Error = LineError;

    fn try_from(line: Line) -> Result<Self, LineError> {
        let Ok(id) = MetafieldId::parse(&line.id) else {
            return BulkRecord::try_from(line).map(Self::Record);
        };
        let missing = |what| LineError(what);
        let parent = line
            .parent_id
            .ok_or(missing("metafield line without __parentId"))?;
        let product_id = ProductId::parse(&parent)
            .map_err(|_| LineError("metafield line whose __parentId is not a product"))?;
        Ok(Self::Metafield(BulkMetafield {
            id,
            product_id,
            namespace: line
                .namespace
                .ok_or(missing("metafield line without namespace"))?,
            key: line.key.ok_or(missing("metafield line without key"))?,
            value: line.value.ok_or(missing("metafield line without value"))?,
            value_type: line
                .value_type
                .ok_or(missing("metafield line without type"))?,
        }))
    }
}

impl<'de> Deserialize<'de> for CatalogLine {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let line = Line::deserialize(deserializer)?;
        Self::try_from(line).map_err(serde::de::Error::custom)
    }
}
