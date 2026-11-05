//! The frozen initial consumer (stage 1): a downstream product-feed job.
//!
//! It does what a merchant-feed integration does with the SDK, through its documented public
//! exports only:
//!
//! - [`list_active_products`] pages through `CatalogProducts` (`status:active`) with
//!   `concord-pagination`'s [`sequential`] chain and the SDK's [`connection_page`] adapter, and
//!   turns each product into a [`FeedProduct`] (one offer: the first variant). A page that comes
//!   back with GraphQL errors fails the listing: a feed is published complete or not at all.
//! - [`submit_feed_export`] submits the bulk selection [`feed_export`] (active products with their
//!   variants; `bulkOperationRunQuery` creates a job on the shop).
//! - [`stream_feed_export`] checks the bulk operation once and, when it completed, streams its
//!   JSON Lines result record by record into the caller's sink as [`FeedLine`]s.
//!
//! This file is not edited after the stage-1 commit.

use std::fmt;

use concord::jsonl::RecordErrors;
use concord::reqwest;
use concord::stream::{OpenError, Position, ReadError};
use concord_example_shopify_sdk::bulk_export::{
    self, BULK_RESULT_OPERATION, BulkRecord, DEFAULT_MAX_LINE_BYTES,
};
use concord_example_shopify_sdk::graphql::UserError;
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::bulk::{BulkOperationQuery, BulkOperationStatus};
use concord_example_shopify_sdk::ops::catalog::{ProductNode, ProductStatus, ProductsQuery};
use concord_example_shopify_sdk::scalars::{BulkOperationId, Decimal, ProductId, ProductVariantId};
use concord_example_shopify_sdk::{
    CatalogExport, Cursor, Data, GraphqlError, PageError, PageMeta, RequestError, SearchQuery,
    ShopContext, connection_page,
};
use concord_pagination::{Budget, Outcome, Page, Resume, sequential};

/// The feed lists one offer per product: only the first variant is requested per product.
const ONE_VARIANT: PageSize = match PageSize::new(1) {
    Ok(size) => size,
    Err(_) => panic!("1 is a valid page size"),
};

/// The products a feed lists: active ones.
fn active() -> SearchQuery {
    SearchQuery::new().term("status", "active")
}

/// One product row of the feed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedProduct {
    /// The product.
    pub id: ProductId,
    /// Title.
    pub title: String,
    /// Handle (the storefront path segment).
    pub handle: String,
    /// Brand.
    pub vendor: String,
    /// Merchant-defined product type.
    pub product_type: String,
    /// The first variant's offer, if the product has a variant.
    pub offer: Option<FeedOffer>,
}

/// The offer of a feed row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedOffer {
    /// The variant.
    pub variant_id: ProductVariantId,
    /// Its SKU, if it has one.
    pub sku: Option<String>,
    /// Its price.
    pub price: Decimal,
}

impl From<ProductNode> for FeedProduct {
    fn from(node: ProductNode) -> Self {
        let offer = node
            .variants
            .nodes
            .into_iter()
            .next()
            .map(|variant| FeedOffer {
                variant_id: variant.id,
                sku: variant.sku,
                price: variant.price,
            });
        Self {
            id: node.id,
            title: node.title,
            handle: node.handle,
            vendor: node.vendor,
            product_type: node.product_type,
            offer,
        }
    }
}

/// The result of [`list_active_products`].
#[derive(Debug)]
pub struct FeedListing {
    /// The rows, in the order Shopify listed them.
    pub products: Vec<FeedProduct>,
    /// Whether the listing reached the end (`Exhausted`) or the page budget (`Bounded`).
    pub outcome: Outcome,
    /// Where to continue a bounded listing.
    pub resume: Option<Resume<Cursor>>,
}

/// Why the feed job stopped. Wraps the SDK's and Concord's errors without stringifying them.
#[derive(Debug)]
pub enum FeedError {
    /// A GraphQL request produced no envelope (transport, status, decode, API version).
    Request(RequestError),
    /// A connection page could not be read.
    Page(PageError),
    /// A page or answer came with GraphQL errors (partial data is not published).
    Partial {
        /// The errors.
        errors: Vec<GraphqlError>,
    },
    /// The answer had no data.
    NoData {
        /// The errors explaining why.
        errors: Vec<GraphqlError>,
    },
    /// The bulk operation id is unknown to the shop.
    UnknownOperation,
    /// Shopify refused to create the bulk job.
    Refused {
        /// Why.
        user_errors: Vec<UserError>,
    },
    /// The result download failed before a response (URL removed from the error).
    Download(reqwest::Error),
    /// The result download was refused (status or media), response unread.
    Open(OpenError),
    /// A result line could not be read; earlier lines were delivered.
    Read(ReadError),
}

impl fmt::Display for FeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(e) => write!(f, "feed: {e}"),
            Self::Page(e) => write!(f, "feed: {e}"),
            Self::Partial { errors } => write!(f, "feed: {} GraphQL error(s)", errors.len()),
            Self::NoData { errors } => {
                write!(f, "feed: no data ({} GraphQL error(s))", errors.len())
            }
            Self::UnknownOperation => f.write_str("feed: unknown bulk operation"),
            Self::Refused { user_errors } => {
                write!(
                    f,
                    "feed: bulk job refused ({} user error(s))",
                    user_errors.len()
                )
            }
            Self::Download(e) => write!(f, "feed: result download: {e}"),
            Self::Open(e) => write!(f, "feed: {e}"),
            Self::Read(e) => write!(f, "feed: {e}"),
        }
    }
}

impl std::error::Error for FeedError {}

impl From<RequestError> for FeedError {
    fn from(e: RequestError) -> Self {
        Self::Request(e)
    }
}

impl From<OpenError> for FeedError {
    fn from(e: OpenError) -> Self {
        Self::Open(e)
    }
}

impl From<ReadError> for FeedError {
    fn from(e: ReadError) -> Self {
        Self::Read(e)
    }
}

/// One `CatalogProducts` page of active products after `cursor`.
async fn products_page(
    shop: &ShopContext,
    page_size: PageSize,
    cursor: Option<Cursor>,
) -> Result<Page<ProductNode, Cursor, PageMeta>, FeedError> {
    let query = ProductsQuery::new(page_size, ONE_VARIANT)
        .filter(&active())
        .after(cursor.clone());
    let response = shop.send(shop.admin().products(query).prepare());
    let page = connection_page(response, cursor.as_ref(), |data| Some(data.products))
        .await
        .map_err(FeedError::Page)?;
    if !page.meta.errors.is_empty() {
        return Err(FeedError::Partial {
            errors: page.meta.errors,
        });
    }
    Ok(page)
}

/// Lists the shop's active products, `page_size` per page, at most `budget` pages.
///
/// # Errors
///
/// The first page that fails ([`FeedError::Page`], [`FeedError::Partial`]) fails the listing.
pub async fn list_active_products(
    shop: &ShopContext,
    page_size: PageSize,
    budget: Budget,
) -> Result<FeedListing, FeedError> {
    let mut pages = sequential(Resume::Start, budget, |cursor| {
        products_page(shop, page_size, cursor)
    });
    let mut products = Vec::new();
    while let Some(page) = pages.next().await {
        products.extend(page?.items.into_iter().map(FeedProduct::from));
    }
    let report = pages.finish();
    Ok(FeedListing {
        products,
        outcome: report.outcome,
        resume: report.resume,
    })
}

/// The feed's bulk selection: active products with their variants.
pub fn feed_export() -> CatalogExport {
    CatalogExport::products()
        .filter(active())
        .with_variants(true)
}

/// Submits [`feed_export`] as a bulk query (**creates a job** on the shop).
///
/// # Errors
///
/// [`FeedError::Request`], [`FeedError::NoData`], or [`FeedError::Refused`] with Shopify's user
/// errors.
pub async fn submit_feed_export(shop: &ShopContext) -> Result<BulkOperationId, FeedError> {
    let answer = shop
        .send(shop.admin().run_bulk_query(feed_export().run()).prepare())
        .await?;
    let payload = match answer.data {
        Data::Present(data) => data.bulk_operation_run_query,
        Data::Absent | Data::Null => None,
    };
    let Some(payload) = payload else {
        return Err(FeedError::NoData {
            errors: answer.errors,
        });
    };
    match payload.bulk_operation {
        Some(operation) => Ok(operation.id),
        None => Err(FeedError::Refused {
            user_errors: payload.user_errors,
        }),
    }
}

/// One line of the feed export, as handed to the sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedLine {
    /// A product.
    Product {
        /// The product.
        id: ProductId,
        /// Title.
        title: String,
        /// Handle.
        handle: String,
        /// Brand.
        vendor: String,
        /// Status.
        status: ProductStatus,
    },
    /// An offer (a variant) of a product listed before it.
    Offer {
        /// The variant.
        variant_id: ProductVariantId,
        /// Its product.
        product_id: ProductId,
        /// SKU.
        sku: Option<String>,
        /// Price.
        price: Decimal,
        /// Sellable quantity, if tracked.
        quantity: Option<i64>,
    },
}

/// What [`stream_feed_export`] streamed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamReport {
    /// Product lines delivered.
    pub products: u64,
    /// Offer lines delivered.
    pub offers: u64,
    /// Where the reader stood at the end.
    pub position: Position,
}

/// How [`stream_feed_export`] ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportState {
    /// The operation has not completed (yet): nothing was downloaded.
    NotReady(BulkOperationStatus),
    /// Completed without results (`url: null`).
    Empty,
    /// Completed; the result was streamed to its end.
    Streamed(StreamReport),
}

/// Checks bulk operation `id` once; when it completed, streams its result into `sink`.
///
/// # Errors
///
/// [`FeedError`]: request, unknown operation, refused or failed download, or the first line that
/// cannot be read (malformed lines are not skipped; earlier lines stay delivered).
pub async fn stream_feed_export(
    shop: &ShopContext,
    id: &BulkOperationId,
    mut sink: impl FnMut(FeedLine),
) -> Result<ExportState, FeedError> {
    let answer = shop
        .send(
            shop.admin()
                .bulk_operation(BulkOperationQuery { id: id.clone() })
                .prepare(),
        )
        .await?;
    let operation = match answer.data {
        Data::Present(data) => data.bulk_operation.ok_or(FeedError::UnknownOperation)?,
        Data::Absent | Data::Null => {
            return Err(FeedError::NoData {
                errors: answer.errors,
            });
        }
    };
    if operation.status != BulkOperationStatus::Completed {
        return Ok(ExportState::NotReady(operation.status));
    }
    let Some(url) = operation.url else {
        return Ok(ExportState::Empty);
    };
    let response = shop
        .http()
        .execute(shop.result_download(&url))
        .await
        .map_err(|e| FeedError::Download(e.without_url()))?;
    let mut records = bulk_export::result_reader(DEFAULT_MAX_LINE_BYTES, RecordErrors::Stop)
        .open_for::<BulkRecord>(response, BULK_RESULT_OPERATION)?;
    let mut report = StreamReport::default();
    while let Some(record) = records.next().await {
        let line = match record? {
            BulkRecord::Product(product) => {
                report.products += 1;
                FeedLine::Product {
                    id: product.id,
                    title: product.title,
                    handle: product.handle,
                    vendor: product.vendor,
                    status: product.status,
                }
            }
            BulkRecord::Variant(variant) => {
                report.offers += 1;
                FeedLine::Offer {
                    variant_id: variant.id,
                    product_id: variant.product_id,
                    sku: variant.sku,
                    price: variant.price,
                    quantity: variant.inventory_quantity,
                }
            }
        };
        sink(line);
    }
    report.position = records.position();
    Ok(ExportState::Streamed(report))
}
