//! Catalog reads: products with their first variants, one product by id, a product's further
//! variant pages, collections with their first member products, and a collection's further
//! member pages.

use serde::{Deserialize, Serialize};
use std::fmt;

use super::{PageSize, open_enum};
use crate::connection::Connection;
use crate::graphql::{Included, Operation, OperationKind, Var, nullable};
use crate::scalars::{
    CollectionId, Cursor, DateTime, Decimal, InventoryItemId, ProductId, ProductVariantId,
};
use crate::search::SearchQuery;

open_enum! {
    /// `ProductStatus`.
    ProductStatus {
        /// Sellable.
        Active = "ACTIVE",
        /// Archived.
        Archived = "ARCHIVED",
        /// Draft.
        Draft = "DRAFT",
        /// Unlisted.
        Unlisted = "UNLISTED",
    }
}

open_enum! {
    /// `CountPrecision`.
    CountPrecision {
        /// The count is exact.
        Exact = "EXACT",
        /// The count is a lower bound.
        AtLeast = "AT_LEAST",
    }
}

/// `Count { count precision }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Count {
    /// The count.
    pub count: u64,
    /// Whether it is exact.
    pub precision: CountPrecision,
}

/// The variant selection shared by [`ProductsQuery`], [`ProductQuery`] and
/// [`ProductVariantsQuery`] (a macro so `concat!` can splice it into the documents).
macro_rules! variant_fields {
    () => {
        "id title sku price compareAtPrice inventoryQuantity position inventoryItem { id tracked }"
    };
}

/// A product variant as selected by the catalog operations.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantNode {
    /// `ID!`
    pub id: ProductVariantId,
    /// `String!`
    pub title: String,
    /// `String` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub sku: Option<String>,
    /// `Money!`
    pub price: Decimal,
    /// `Money` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub compare_at_price: Option<Decimal>,
    /// `Int` (nullable): total sellable quantity across locations.
    #[serde(deserialize_with = "nullable")]
    pub inventory_quantity: Option<i64>,
    /// `Int!`, from 1.
    pub position: u32,
    /// `InventoryItem!`
    pub inventory_item: VariantInventoryItem,
}

/// `inventoryItem { id tracked }` of a variant.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct VariantInventoryItem {
    /// `ID!`
    pub id: InventoryItemId,
    /// `Boolean!`
    pub tracked: bool,
}

/// `products(first:, after:, query:, sortKey: ID)` with each product's first variants.
///
/// Variables: `first` (page size), `after` (the previous `endCursor`; absent for the first
/// page), `query` (search syntax; absent: every product), `variantsFirst`, and
/// `withOnlineStoreUrl` (absent: `false`, the document's default) which includes each product's
/// `onlineStoreUrl` ([`with_online_store_url`](Self::with_online_store_url)).
///
/// `Debug` withholds `query`, which [`filter`](Self::filter) fills from a [`SearchQuery`] whose own
/// `Debug` withholds its terms.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductsQuery {
    /// Products per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
    /// A search filter.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub query: Var<String>,
    /// Variants per product in this page (the rest: [`ProductVariantsQuery`]).
    pub variants_first: PageSize,
    /// Whether to include `onlineStoreUrl` (`@include(if: $withOnlineStoreUrl)`); absent means
    /// the document's default, `false`.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub with_online_store_url: Var<bool>,
}

impl fmt::Debug for ProductsQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let query = match &self.query {
            Var::Absent => "Absent",
            Var::Null => "Null",
            Var::Value(_) => "Value(<redacted>)",
        };
        f.debug_struct("ProductsQuery")
            .field("first", &self.first)
            .field("after", &self.after)
            .field("query", &format_args!("{query}"))
            .field("variants_first", &self.variants_first)
            .field("with_online_store_url", &self.with_online_store_url)
            .finish()
    }
}

impl ProductsQuery {
    /// The first page of every product, `first` per page, `variants_first` variants each.
    pub fn new(first: PageSize, variants_first: PageSize) -> Self {
        Self {
            first,
            after: Var::Absent,
            query: Var::Absent,
            variants_first,
            with_online_store_url: Var::Absent,
        }
    }

    /// Also selects each product's `onlineStoreUrl` ([`ProductNode::online_store_url`]); without
    /// it the variable is omitted and the field is not in the response.
    #[must_use]
    pub fn with_online_store_url(mut self) -> Self {
        self.with_online_store_url = Var::Value(true);
        self
    }

    /// Continues after `cursor` (`None`: the first page, `after` omitted).
    #[must_use]
    pub fn after(mut self, cursor: Option<Cursor>) -> Self {
        self.after = Var::or_absent(cursor);
        self
    }

    /// Restricts to products matching `query` (an empty query is omitted).
    #[must_use]
    pub fn filter(mut self, query: &SearchQuery) -> Self {
        self.query = if query.is_empty() {
            Var::Absent
        } else {
            Var::Value(query.as_str().to_owned())
        };
        self
    }
}

impl Operation for ProductsQuery {
    const NAME: &'static str = "CatalogProducts";
    const DOCUMENT: &'static str = concat!(
        "query CatalogProducts($first: Int!, $after: String, $query: String, $variantsFirst: Int!, \
         $withOnlineStoreUrl: Boolean! = false) { \
         products(first: $first, after: $after, query: $query, sortKey: ID) { \
         nodes { id title handle status vendor productType \
         onlineStoreUrl @include(if: $withOnlineStoreUrl) tracksInventory totalInventory updatedAt \
         variants(first: $variantsFirst) { nodes { ",
        variant_fields!(),
        " } pageInfo { hasNextPage endCursor } } } \
         pageInfo { hasNextPage endCursor } } }",
    );
    const KIND: OperationKind = OperationKind::Query;
    type Data = ProductsData;
}

/// `data` of [`ProductsQuery`].
#[derive(Debug, Deserialize)]
pub struct ProductsData {
    /// `ProductConnection!`
    pub products: Connection<ProductNode>,
}

/// A product as listed by [`ProductsQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductNode {
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
    /// `String!`
    pub product_type: String,
    /// `URL` (nullable), only when requested ([`ProductsQuery::with_online_store_url`]): the
    /// product's page on the online store; `null` when the product is not published to the online
    /// store sales channel.
    #[serde(default)]
    pub online_store_url: Included<String>,
    /// `Boolean!`
    pub tracks_inventory: bool,
    /// `Int!`
    pub total_inventory: i64,
    /// `DateTime!`
    pub updated_at: DateTime,
    /// The first variants (`ProductVariantConnection!`).
    pub variants: Connection<VariantNode>,
}

/// `product(id:)` with details, its first variants and its first collections.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductQuery {
    /// The product.
    pub id: ProductId,
    /// Variants to include.
    pub variants_first: PageSize,
    /// Collections to include.
    pub collections_first: PageSize,
}

impl Operation for ProductQuery {
    const NAME: &'static str = "CatalogProduct";
    const DOCUMENT: &'static str = concat!(
        "query CatalogProduct($id: ID!, $variantsFirst: Int!, $collectionsFirst: Int!) { \
         product(id: $id) { id title handle status vendor productType tags totalInventory \
         variantsCount { count precision } \
         variants(first: $variantsFirst) { nodes { ",
        variant_fields!(),
        " } pageInfo { hasNextPage endCursor } } \
         collections(first: $collectionsFirst) { nodes { id title handle } \
         pageInfo { hasNextPage endCursor } } } }",
    );
    const KIND: OperationKind = OperationKind::Query;
    type Data = ProductData;
}

/// `data` of [`ProductQuery`].
#[derive(Debug, Deserialize)]
pub struct ProductData {
    /// `Product` (nullable): `None` for an unknown id.
    #[serde(deserialize_with = "nullable")]
    pub product: Option<ProductDetail>,
}

/// A product as selected by [`ProductQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
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
    /// `String!`
    pub product_type: String,
    /// `[String!]!`
    pub tags: Vec<String>,
    /// `Int!`
    pub total_inventory: i64,
    /// `Count` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub variants_count: Option<Count>,
    /// `ProductVariantConnection!`
    pub variants: Connection<VariantNode>,
    /// `CollectionConnection!`
    pub collections: Connection<CollectionRef>,
}

/// `{ id title handle }` of a collection.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CollectionRef {
    /// `ID!`
    pub id: CollectionId,
    /// `String!`
    pub title: String,
    /// `String!`
    pub handle: String,
}

/// `product(id:) { variants(first:, after:) }`: the nested variant connection of one product,
/// continued from a dependent cursor.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductVariantsQuery {
    /// The product.
    pub id: ProductId,
    /// Variants per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
}

impl ProductVariantsQuery {
    /// The variants of `id`, `first` per page, after `cursor` (`None`: from the start).
    pub fn new(id: ProductId, first: PageSize, cursor: Option<Cursor>) -> Self {
        Self {
            id,
            first,
            after: Var::or_absent(cursor),
        }
    }
}

impl Operation for ProductVariantsQuery {
    const NAME: &'static str = "ProductVariants";
    const DOCUMENT: &'static str = concat!(
        "query ProductVariants($id: ID!, $first: Int!, $after: String) { \
         product(id: $id) { id variants(first: $first, after: $after) { nodes { ",
        variant_fields!(),
        " } pageInfo { hasNextPage endCursor } } } }",
    );
    const KIND: OperationKind = OperationKind::Query;
    type Data = ProductVariantsData;
}

/// `data` of [`ProductVariantsQuery`].
#[derive(Debug, Deserialize)]
pub struct ProductVariantsData {
    /// `Product` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub product: Option<ProductVariants>,
}

/// The product of [`ProductVariantsQuery`].
#[derive(Debug, Deserialize)]
pub struct ProductVariants {
    /// `ID!`
    pub id: ProductId,
    /// `ProductVariantConnection!`
    pub variants: Connection<VariantNode>,
}

/// `collections(first:, after:, query:, sortKey: ID)` with each collection's first member
/// product ids.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionsQuery {
    /// Collections per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
    /// A search filter.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub query: Var<String>,
    /// Member products per collection in this page (the rest: [`CollectionProductsQuery`]).
    pub products_first: PageSize,
}

impl CollectionsQuery {
    /// The first page of every collection.
    pub fn new(first: PageSize, products_first: PageSize) -> Self {
        Self {
            first,
            after: Var::Absent,
            query: Var::Absent,
            products_first,
        }
    }

    /// Continues after `cursor` (`None`: the first page).
    #[must_use]
    pub fn after(mut self, cursor: Option<Cursor>) -> Self {
        self.after = Var::or_absent(cursor);
        self
    }
}

impl Operation for CollectionsQuery {
    const NAME: &'static str = "CatalogCollections";
    const DOCUMENT: &'static str = "query CatalogCollections($first: Int!, $after: String, \
         $query: String, $productsFirst: Int!) { \
         collections(first: $first, after: $after, query: $query, sortKey: ID) { \
         nodes { id title handle updatedAt productsCount { count precision } \
         products(first: $productsFirst) { nodes { id } pageInfo { hasNextPage endCursor } } } \
         pageInfo { hasNextPage endCursor } } }";
    const KIND: OperationKind = OperationKind::Query;
    type Data = CollectionsData;
}

/// `data` of [`CollectionsQuery`].
#[derive(Debug, Deserialize)]
pub struct CollectionsData {
    /// `CollectionConnection!`
    pub collections: Connection<CollectionNode>,
}

/// A collection as listed by [`CollectionsQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionNode {
    /// `ID!`
    pub id: CollectionId,
    /// `String!`
    pub title: String,
    /// `String!`
    pub handle: String,
    /// `DateTime!`
    pub updated_at: DateTime,
    /// `Count` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub products_count: Option<Count>,
    /// The first member products (`ProductConnection!`).
    pub products: Connection<ProductRef>,
}

/// `{ id }` of a product.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct ProductRef {
    /// `ID!`
    pub id: ProductId,
}

/// `collection(id:) { products(first:, after:) }`: a collection's further member pages.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionProductsQuery {
    /// The collection.
    pub id: CollectionId,
    /// Products per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
}

impl CollectionProductsQuery {
    /// The member products of `id` after `cursor` (`None`: from the start).
    pub fn new(id: CollectionId, first: PageSize, cursor: Option<Cursor>) -> Self {
        Self {
            id,
            first,
            after: Var::or_absent(cursor),
        }
    }
}

impl Operation for CollectionProductsQuery {
    const NAME: &'static str = "CollectionProducts";
    const DOCUMENT: &'static str = "query CollectionProducts($id: ID!, $first: Int!, $after: String) { \
         collection(id: $id) { id products(first: $first, after: $after) { nodes { id } \
         pageInfo { hasNextPage endCursor } } } }";
    const KIND: OperationKind = OperationKind::Query;
    type Data = CollectionProductsData;
}

/// `data` of [`CollectionProductsQuery`].
#[derive(Debug, Deserialize)]
pub struct CollectionProductsData {
    /// `Collection` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub collection: Option<CollectionProducts>,
}

/// The collection of [`CollectionProductsQuery`].
#[derive(Debug, Deserialize)]
pub struct CollectionProducts {
    /// `ID!`
    pub id: CollectionId,
    /// `ProductConnection!`
    pub products: Connection<ProductRef>,
}
