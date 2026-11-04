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

use std::fmt;

use concord::jsonl::{JsonLinesOptions, JsonLinesReader, MediaPolicy, RecordErrors};
use serde::Deserialize;
use serde::de::Deserializer;

use crate::graphql::string_literal;
use crate::ops::bulk::RunBulkQuery;
use crate::ops::catalog::{ProductStatus, VariantInventoryItem};
use crate::scalars::{DateTime, Decimal, ProductId, ProductVariantId};
use crate::search::SearchQuery;

/// The operation label of result-download errors (`ReadError::operation`, `OpenError::operation`).
pub const BULK_RESULT_OPERATION: &str = "shopify: bulk result";

/// The default per-line limit of [`result_reader`]: 1 MiB, far above the selection's lines
/// (a few hundred bytes) and far below the reader's 16 MiB default.
pub const DEFAULT_MAX_LINE_BYTES: usize = 1024 * 1024;

/// What a catalog export selects: products (optionally filtered), optionally with their variants
/// as child lines.
#[derive(Clone, Debug, Default)]
pub struct CatalogExport {
    filter: SearchQuery,
    variants: bool,
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
        format!(
            "{{ products{arguments} {{ edges {{ node {{ id title handle status vendor updatedAt{variants} }} }} }} }}"
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
