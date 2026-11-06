//! The updated consumer (stage 3): the same feed job using the SDK's new capabilities.
//!
//! - [`list_published_products`] asks `CatalogProducts` for each product's `onlineStoreUrl`
//!   (the feed's product link): a product with a link is listed, one whose link is `null` (not
//!   published to the online store) is set aside, and an answer without the member although it
//!   was requested is refused ([`UpdatedFeedError::LinkNotIncluded`]).
//! - [`attribute_export`] adds the products' Google Shopping metafields to the frozen feed
//!   selection, and [`stream_attribute_export`] reads the result as [`CatalogLine`]s: products and
//!   offers go through the frozen consumer's own types, each metafield becomes an attribute of the
//!   product named by its `__parentId`.
//!
//! The frozen consumer ([`crate::before`]) stays beside it, unchanged; its types are reused here.

use std::fmt;

use concord::jsonl::{JsonLinesRecords, RecordErrors};
use concord_example_shopify_sdk::bulk_export::{
    self, BulkRecord, CatalogLine, DEFAULT_MAX_LINE_BYTES,
};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::bulk::{BulkOperationQuery, BulkOperationStatus};
use concord_example_shopify_sdk::ops::catalog::{ProductNode, ProductsQuery};
use concord_example_shopify_sdk::scalars::{BulkOperationId, ProductId};
use concord_example_shopify_sdk::{
    CatalogExport, Cursor, Data, Included, PageMeta, SearchQuery, ShopContext, connection_page,
};
use concord_pagination::{Budget, Outcome, Page, Resume, sequential};
use serde::de::DeserializeOwned;

use crate::before::{FeedError, FeedLine, FeedProduct, StreamReport, feed_export};

/// The metafield namespace of Google Shopping attributes (gender, age group, product category).
pub const FEED_NAMESPACE: &str = "mm-google-shopping";

/// One offer per product, as in the frozen consumer.
const ONE_VARIANT: PageSize = match PageSize::new(1) {
    Ok(size) => size,
    Err(_) => panic!("1 is a valid page size"),
};

/// A product the feed lists, with its storefront link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListedProduct {
    /// The row, as the frozen consumer builds it.
    pub product: FeedProduct,
    /// `onlineStoreUrl`.
    pub link: String,
}

/// The result of [`list_published_products`].
#[derive(Debug)]
pub struct PublishedListing {
    /// Products published to the online store, in listing order.
    pub listed: Vec<ListedProduct>,
    /// Active products whose `onlineStoreUrl` is `null` (not published to the online store).
    pub unpublished: Vec<ProductId>,
    /// Whether the listing reached the end or the page budget.
    pub outcome: Outcome,
    /// Where to continue a bounded listing.
    pub resume: Option<Resume<Cursor>>,
}

/// Why the updated feed job stopped.
#[derive(Debug)]
pub enum UpdatedFeedError {
    /// A failure the frozen consumer already knows.
    Feed(FeedError),
    /// `onlineStoreUrl` was requested but the answer does not carry it (an answer to an older
    /// document, a replayed recording): never read as "unpublished".
    LinkNotIncluded {
        /// The first product without the member.
        product: ProductId,
    },
}

impl fmt::Display for UpdatedFeedError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Feed(e) => fmt::Display::fmt(e, f),
            Self::LinkNotIncluded { product } => write!(
                f,
                "feed: onlineStoreUrl was requested but is missing for {product}"
            ),
        }
    }
}

impl std::error::Error for UpdatedFeedError {}

impl From<FeedError> for UpdatedFeedError {
    fn from(e: FeedError) -> Self {
        Self::Feed(e)
    }
}

/// One `CatalogProducts` page of active products after `cursor`, with their links.
async fn linked_page(
    shop: &ShopContext,
    page_size: PageSize,
    cursor: Option<Cursor>,
) -> Result<Page<ProductNode, Cursor, PageMeta>, FeedError> {
    let query = ProductsQuery::new(page_size, ONE_VARIANT)
        .filter(&SearchQuery::new().term("status", "active"))
        .after(cursor.clone())
        .with_online_store_url();
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

/// Lists the shop's active products with their storefront links, `page_size` per page, at most
/// `budget` pages.
///
/// # Errors
///
/// The first failing page, or [`UpdatedFeedError::LinkNotIncluded`].
pub async fn list_published_products(
    shop: &ShopContext,
    page_size: PageSize,
    budget: Budget,
) -> Result<PublishedListing, UpdatedFeedError> {
    let mut pages = sequential(Resume::Start, budget, |cursor| {
        linked_page(shop, page_size, cursor)
    });
    let mut listed = Vec::new();
    let mut unpublished = Vec::new();
    while let Some(page) = pages.next().await {
        for mut node in page?.items {
            match std::mem::take(&mut node.online_store_url) {
                Included::Value(link) => listed.push(ListedProduct {
                    product: FeedProduct::from(node),
                    link,
                }),
                Included::Null => unpublished.push(node.id),
                Included::NotIncluded => {
                    return Err(UpdatedFeedError::LinkNotIncluded { product: node.id });
                }
            }
        }
    }
    let report = pages.finish();
    Ok(PublishedListing {
        listed,
        unpublished,
        outcome: report.outcome,
        resume: report.resume,
    })
}

/// The frozen feed selection plus the products' [`FEED_NAMESPACE`] metafields.
pub fn attribute_export() -> CatalogExport {
    feed_export().with_metafields(Some(FEED_NAMESPACE))
}

/// Submits [`attribute_export`] as a bulk query (**creates a job** on the shop).
///
/// # Errors
///
/// As the frozen `submit_feed_export`.
pub async fn submit_attribute_export(shop: &ShopContext) -> Result<BulkOperationId, FeedError> {
    let answer = shop
        .send(
            shop.admin()
                .run_bulk_query(attribute_export().run())
                .prepare(),
        )
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

/// One line of the attribute export, as handed to the sink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeLine {
    /// A product or offer line, as the frozen consumer delivers it.
    Feed(FeedLine),
    /// A metafield of a product listed before it.
    Attribute {
        /// The product (`__parentId`).
        product_id: ProductId,
        /// The metafield key within [`FEED_NAMESPACE`].
        key: String,
        /// Its value as text.
        value: String,
        /// Its metafield type name.
        value_type: String,
    },
}

/// What [`stream_attribute_export`] streamed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttributeReport {
    /// Product and offer lines (and the reader's position at the end).
    pub feed: StreamReport,
    /// Attribute lines delivered.
    pub attributes: u64,
    /// Lines of a kind this consumer does not know (a later SDK's child connection), skipped.
    pub unknown: u64,
}

/// How [`stream_attribute_export`] ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttributeState {
    /// The operation has not completed (yet): nothing was downloaded.
    NotReady(BulkOperationStatus),
    /// Completed without results.
    Empty,
    /// Completed; the result was streamed to its end.
    Streamed(AttributeReport),
}

/// The completed result of operation `id`, opened as records of `T`.
enum Opened<T> {
    NotReady(BulkOperationStatus),
    Empty,
    /// Boxed: the reader state is a few hundred bytes (clippy `large_enum_variant`).
    Records(Box<JsonLinesRecords<T>>),
}

/// Checks bulk operation `id` once and opens its result (the frozen consumer's steps, with the
/// record type as a parameter).
async fn open_result<T: DeserializeOwned>(
    shop: &ShopContext,
    id: &BulkOperationId,
) -> Result<Opened<T>, FeedError> {
    let query = BulkOperationQuery { id: id.clone() };
    let answer = shop
        .send(shop.admin().bulk_operation(query).prepare())
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
        return Ok(Opened::NotReady(operation.status));
    }
    let Some(url) = operation.url else {
        return Ok(Opened::Empty);
    };
    let response = shop
        .http()
        .execute(shop.result_download(&url))
        .await
        .map_err(|e| FeedError::Download(e.without_url()))?;
    let reader = bulk_export::result_reader(DEFAULT_MAX_LINE_BYTES, RecordErrors::Stop);
    let records = bulk_export::open_result::<T>(&reader, response)?;
    Ok(Opened::Records(Box::new(records)))
}

/// A product or variant line as the frozen consumer's [`FeedLine`].
fn feed_line(record: BulkRecord, report: &mut StreamReport) -> FeedLine {
    match record {
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
    }
}

/// Checks bulk operation `id` (an [`attribute_export`]) once; when it completed, streams its
/// result into `sink`.
///
/// # Errors
///
/// As the frozen `stream_feed_export`.
pub async fn stream_attribute_export(
    shop: &ShopContext,
    id: &BulkOperationId,
    mut sink: impl FnMut(AttributeLine),
) -> Result<AttributeState, FeedError> {
    let mut records = match open_result::<CatalogLine>(shop, id).await? {
        Opened::NotReady(status) => return Ok(AttributeState::NotReady(status)),
        Opened::Empty => return Ok(AttributeState::Empty),
        Opened::Records(records) => records,
    };
    let mut report = AttributeReport::default();
    while let Some(line) = records.next().await {
        let line = match line? {
            CatalogLine::Record(record) => AttributeLine::Feed(feed_line(record, &mut report.feed)),
            CatalogLine::Metafield(metafield) => {
                report.attributes += 1;
                AttributeLine::Attribute {
                    product_id: metafield.product_id,
                    key: metafield.key,
                    value: metafield.value,
                    value_type: metafield.value_type,
                }
            }
            // `CatalogLine` is non-exhaustive: a kind this consumer does not know is counted.
            _ => {
                report.unknown += 1;
                continue;
            }
        };
        sink(line);
    }
    report.feed.position = records.position();
    Ok(AttributeState::Streamed(report))
}
