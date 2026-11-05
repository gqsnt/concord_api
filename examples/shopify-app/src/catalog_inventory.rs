//! SH-A — catalog and inventory reconciliation for one shop and one location.
//!
//! # What it does
//!
//! Four connection traversals run one after the other (Shopify's cost model is per app and shop;
//! nothing here runs concurrently), each a `concord_pagination::sequential` chain over the SDK's
//! [`connection_page`] adapter, so each request needs the previous page's `endCursor`:
//!
//! 1. `Locations` — finds the target location (the one asked for, or the only active one);
//! 2. `CatalogProducts` — products with their first variants; a product whose variant
//!    connection has more pages gets a **nested** traversal of `ProductVariants`, started at the
//!    nested `endCursor`;
//! 3. `CatalogCollections` (optional) — collections with their first members, and nested
//!    `CollectionProducts` traversals for the rest;
//! 4. `LocationInventory` — the location's inventory levels with their items and quantities.
//!
//! Then it compares the catalog with the levels ([`Discrepancy`]).
//!
//! # Policies (this workflow's, not the decoder's)
//!
//! - **Partial data** ([`PartialData`]): a page with data *and* errors is kept with its errors
//!   recorded in [`ReconcileReport::accepted_errors`] (`Accept`), or refused as a failed page
//!   whose cursor is the resume point (`Reject`). A page without its connection (data absent or
//!   `null`, a `null` parent) is always a failure.
//! - **Failures**: the first failed traversal (throttled, refused, inconsistent, not decodable)
//!   ends the run; later stages are [`Stage::NotRun`]. Nothing is retried and nothing waits: a
//!   `THROTTLED` failure keeps the bucket state so the caller can schedule the resume. A failed
//!   nested traversal also ends its parent traversal in the middle of a page: the parent's
//!   summary is [`Outcome::Stopped`], and its resume point is that page, whose later parents were
//!   not processed (see [`TraversalSummary`]).
//! - **Budgets**: every traversal (nested ones included) has the same page budget; a bounded
//!   traversal is not a failure, the run continues and the report says what is incomplete.
//! - **Completeness**: a check runs only when its inputs are complete (see [`Discrepancy`]);
//!   the report's [`ReconcileStatus`] is `Complete` only if every enabled traversal was exhausted
//!   without accepted errors.
//!
//! Memory: the catalog and the levels seen are kept (to join them), bounded by the page budgets
//! times the page sizes. This is a reconciliation, not a stream.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use concord::Prepared;
use concord_example_shopify_sdk::connection::{Connection, PageMeta};
use concord_example_shopify_sdk::ops::PageSize;
use concord_example_shopify_sdk::ops::catalog::{
    CollectionProductsQuery, CollectionsQuery, ProductStatus, ProductVariantsQuery, ProductsQuery,
    VariantNode,
};
use concord_example_shopify_sdk::ops::inventory::{
    LocationInventoryQuery, LocationNode, LocationsQuery, QuantityName,
};
use concord_example_shopify_sdk::scalars::{
    CollectionId, InventoryItemId, InventoryLevelId, LocationId, ProductId, ProductVariantId,
};
use concord_example_shopify_sdk::{
    Cursor, GraphqlError, GraphqlResponse, PageError, QueryCost, SearchQuery, ShopContext,
    ShopDomain, ThrottleStatus, connection_page,
};
use concord_pagination::{Budget, Outcome, Page, Resume, SequentialReport, sequential};

use crate::cost::CostLedger;

/// What to do with a page that has data and errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartialData {
    /// Keep the data, record the errors in the report.
    Accept,
    /// Refuse the page: the traversal fails there, with the errors, and resumes at that page.
    Reject,
}

/// Page sizes per connection.
#[derive(Clone, Copy, Debug)]
pub struct PageSizes {
    /// `locations(first:)`.
    pub locations: PageSize,
    /// `products(first:)`.
    pub products: PageSize,
    /// `variants(first:)`, in the product page and in nested pages.
    pub variants: PageSize,
    /// `collections(first:)`.
    pub collections: PageSize,
    /// `products(first:)` of a collection.
    pub collection_products: PageSize,
    /// `inventoryLevels(first:)`.
    pub inventory_levels: PageSize,
}

impl Default for PageSizes {
    fn default() -> Self {
        let size = |n| PageSize::new(n).expect("a valid page size");
        Self {
            locations: size(50),
            products: size(50),
            variants: size(50),
            collections: size(50),
            collection_products: size(100),
            inventory_levels: size(100),
        }
    }
}

/// The inputs of one reconciliation.
#[derive(Clone, Debug)]
pub struct ReconcileRequest {
    /// The location to reconcile; `None`: the only active location (refused if there are
    /// several).
    pub location: Option<LocationId>,
    /// Products to include (empty: all). With a filter, "a level without a variant" is not
    /// checked.
    pub product_filter: SearchQuery,
    /// Whether collections are traversed (for the "in no collection" check).
    pub collections: bool,
    /// Page sizes.
    pub page_sizes: PageSizes,
    /// Pages per traversal (each nested traversal included).
    pub pages_per_traversal: Budget,
    /// Nested traversals allowed per kind (variant pages, collection members); further parents
    /// with more pages are counted as skipped.
    pub nested_traversals: usize,
    /// The partial-data policy.
    pub partial_data: PartialData,
}

impl ReconcileRequest {
    /// The defaults: the only active location, every product, collections included, 20 pages
    /// per traversal, 50 nested traversals, partial data accepted.
    pub fn new(pages_per_traversal: Budget) -> Self {
        Self {
            location: None,
            product_filter: SearchQuery::new(),
            collections: true,
            page_sizes: PageSizes::default(),
            pages_per_traversal,
            nested_traversals: 50,
            partial_data: PartialData::Accept,
        }
    }
}

/// Why a page of a traversal failed.
pub enum SyncFailure {
    /// The adapter's failure (request, no connection, inconsistent `pageInfo`).
    Page(PageError),
    /// [`PartialData::Reject`] refused a page that had data and errors.
    PartialRejected {
        /// The page's errors.
        errors: Vec<GraphqlError>,
        /// The page's cost report.
        cost: Option<QueryCost>,
    },
}

impl SyncFailure {
    /// The GraphQL errors of the failed page.
    pub fn graphql_errors(&self) -> &[GraphqlError] {
        match self {
            Self::Page(e) => e.graphql_errors(),
            Self::PartialRejected { errors, .. } => errors,
        }
    }

    /// Whether the failure is a `THROTTLED` answer.
    pub fn is_throttled(&self) -> bool {
        matches!(self, Self::Page(e) if e.is_throttled())
    }

    /// The bucket state reported with the failed page.
    pub fn throttle_status(&self) -> Option<ThrottleStatus> {
        self.cost().map(|c| c.throttle_status)
    }

    fn cost(&self) -> Option<&QueryCost> {
        match self {
            Self::Page(e) => e.cost(),
            Self::PartialRejected { cost, .. } => cost.as_ref(),
        }
    }

    fn decoded(&self) -> bool {
        !matches!(self, Self::Page(PageError::Request(_)))
    }
}

impl fmt::Display for SyncFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(e) => fmt::Display::fmt(e, f),
            Self::PartialRejected { errors, .. } => {
                write!(f, "page refused: data with {} error(s)", errors.len())
            }
        }
    }
}

impl fmt::Debug for SyncFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(e) => f.debug_tuple("Page").field(e).finish(),
            Self::PartialRejected { errors, cost } => f
                .debug_struct("PartialRejected")
                .field("errors", errors)
                .field("cost", cost)
                .finish(),
        }
    }
}

impl std::error::Error for SyncFailure {}

/// How one traversal ended.
///
/// `Stopped` is reported only by `CatalogProducts` and `CatalogCollections`, when the nested
/// traversal of one of a page's parents failed: the parents after it on that page were not
/// processed (not counted, not compared). The page being consumed is then the resume point,
/// `failure` is `None`, and the nested failure is in [`ReconcileReport::variant_pages`] or
/// [`ReconcileReport::member_pages`] (`incomplete`).
#[derive(Debug)]
pub struct TraversalSummary {
    /// The pagination outcome (`Exhausted`, `Bounded`, `Failed`, `Stopped`).
    pub outcome: Outcome,
    /// Pages received successfully (a stopped traversal's interrupted page included).
    pub pages: u64,
    /// Where to resume: the failed page after a failure, the next page when bounded, the
    /// interrupted page when stopped; `None` when exhausted.
    pub resume: Option<Resume<Cursor>>,
    /// The failure, when `outcome` is `Failed` (a stopped traversal's failure is the nested one).
    pub failure: Option<SyncFailure>,
}

impl TraversalSummary {
    fn new(report: SequentialReport<Cursor>, failure: Option<SyncFailure>) -> Self {
        Self {
            outcome: report.outcome,
            pages: report.progress.succeeded,
            resume: report.resume,
            failure,
        }
    }

    /// A traversal whose page `interrupted` (its checkpoint before `next()`) was being consumed
    /// when a nested traversal failed. Whatever the chain itself would report (its position has
    /// moved past that page, possibly to the end), the page is where to resume.
    fn interrupted(report: SequentialReport<Cursor>, interrupted: Option<Resume<Cursor>>) -> Self {
        Self {
            outcome: Outcome::Stopped,
            pages: report.progress.succeeded,
            resume: interrupted,
            failure: None,
        }
    }

    /// Whether the provider had no more pages and every page succeeded.
    pub fn is_exhausted(&self) -> bool {
        self.outcome.is_success()
    }
}

/// One top-level traversal.
#[derive(Debug)]
pub enum Stage {
    /// It ran.
    Ran(TraversalSummary),
    /// It did not run.
    NotRun(NotRun),
}

impl Stage {
    /// The summary, if it ran.
    pub fn summary(&self) -> Option<&TraversalSummary> {
        match self {
            Self::Ran(summary) => Some(summary),
            Self::NotRun(_) => None,
        }
    }

    fn exhausted(&self) -> bool {
        self.summary().is_some_and(TraversalSummary::is_exhausted)
    }
}

/// Why a stage did not run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotRun {
    /// Disabled by the request.
    Disabled,
    /// An earlier traversal failed.
    EarlierFailure,
    /// No target location: none asked for and not exactly one active location listed.
    NoTargetLocation,
}

/// The nested traversals of one kind (variant pages of products, member pages of collections).
#[derive(Debug)]
pub struct Nested<P> {
    /// Nested traversals run.
    pub traversals: u64,
    /// Pages they received.
    pub pages: u64,
    /// Parents with more pages left untraversed because `nested_traversals` was reached.
    pub skipped: Vec<P>,
    /// Parents whose nested traversal did not end exhausted, with how it ended.
    pub incomplete: Vec<(P, TraversalSummary)>,
}

impl<P> Default for Nested<P> {
    fn default() -> Self {
        Self {
            traversals: 0,
            pages: 0,
            skipped: Vec::new(),
            incomplete: Vec::new(),
        }
    }
}

impl<P> Nested<P> {
    fn complete(&self) -> bool {
        self.skipped.is_empty() && self.incomplete.is_empty()
    }
}

/// A GraphQL error accepted with a page's data.
#[derive(Debug)]
pub struct AcceptedError {
    /// The operation name.
    pub operation: &'static str,
    /// The error.
    pub error: GraphqlError,
}

/// Counts of what was seen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    /// Locations listed.
    pub locations: u64,
    /// Products seen.
    pub products: u64,
    /// Variants seen (first pages and nested pages).
    pub variants: u64,
    /// Collections seen.
    pub collections: u64,
    /// Collection memberships seen.
    pub memberships: u64,
    /// Inventory levels seen at the location.
    pub levels: u64,
    /// Levels whose `unitCost` came back `null` with an `ACCESS_DENIED` error.
    pub unit_costs_withheld: u64,
}

/// A finding of the reconciliation. Each kind states the completeness it requires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Discrepancy {
    /// A tracked variant has no inventory level at the location. Requires the complete level
    /// listing.
    TrackedNotStocked {
        /// The product.
        product: ProductId,
        /// The variant.
        variant: ProductVariantId,
        /// Its inventory item.
        item: InventoryItemId,
    },
    /// A level's `available` quantity is negative.
    NegativeAvailable {
        /// The item.
        item: InventoryItemId,
        /// The quantity.
        available: i64,
    },
    /// With a single active location, a variant's `inventoryQuantity` (all locations) differs
    /// from the location's `available`. Requires the complete location listing.
    QuantityMismatch {
        /// The variant.
        variant: ProductVariantId,
        /// The variant's `inventoryQuantity`.
        variant_quantity: i64,
        /// The location's `available`.
        location_available: i64,
    },
    /// A tracked variant without SKU (`sku: null`).
    MissingSku {
        /// The variant.
        variant: ProductVariantId,
    },
    /// Several variants share a SKU (among the variants seen).
    DuplicateSku {
        /// The SKU.
        sku: String,
        /// The variants, in id order.
        variants: Vec<ProductVariantId>,
    },
    /// A level whose item belongs to no variant seen. Requires the complete, unfiltered catalog.
    LevelWithoutVariant {
        /// The level.
        level: InventoryLevelId,
        /// The item.
        item: InventoryItemId,
    },
    /// An active product in no collection. Requires the complete collection memberships.
    ProductInNoCollection {
        /// The product.
        product: ProductId,
    },
}

/// The overall state of a report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReconcileStatus {
    /// Every enabled traversal was exhausted and no error was accepted.
    Complete,
    /// Every enabled traversal was exhausted; some pages carried errors beside their data.
    CompleteWithErrors,
    /// Some traversal was bounded, failed or did not run: only the checks whose inputs are
    /// complete were made.
    Incomplete,
}

/// The typed result of [`reconcile`].
#[derive(Debug)]
pub struct ReconcileReport {
    /// The shop.
    pub shop: ShopDomain,
    /// The target location: its id and, when it was listed, its node.
    pub location: Option<(LocationId, Option<LocationNode>)>,
    /// `Locations`.
    pub locations: Stage,
    /// `CatalogProducts`.
    pub products: Stage,
    /// `ProductVariants` nested traversals.
    pub variant_pages: Nested<ProductId>,
    /// `CatalogCollections`.
    pub collections: Stage,
    /// `CollectionProducts` nested traversals.
    pub member_pages: Nested<CollectionId>,
    /// `LocationInventory`.
    pub inventory: Stage,
    /// What was seen.
    pub counts: Counts,
    /// The findings, grouped by kind, each kind in id order.
    pub discrepancies: Vec<Discrepancy>,
    /// Errors that came with accepted data ([`PartialData::Accept`]).
    pub accepted_errors: Vec<AcceptedError>,
    /// The cost reports of every decoded response.
    pub cost: CostLedger,
    /// The overall state.
    pub status: ReconcileStatus,
}

struct VariantEntry {
    product: ProductId,
    variant: ProductVariantId,
    sku: Option<String>,
    tracked: bool,
    quantity: Option<i64>,
}

struct LevelEntry {
    level: InventoryLevelId,
    available: Option<i64>,
}

#[derive(Default)]
struct State {
    counts: Counts,
    cost: CostLedger,
    accepted: Vec<AcceptedError>,
    products: BTreeMap<ProductId, ProductStatus>,
    variants: BTreeMap<InventoryItemId, VariantEntry>,
    collected: BTreeSet<ProductId>,
    levels: BTreeMap<InventoryItemId, LevelEntry>,
    locations: Vec<LocationNode>,
    variant_pages: Nested<ProductId>,
    member_pages: Nested<CollectionId>,
    stop: bool,
}

impl State {
    fn page<N>(&mut self, operation: &'static str, page: &mut Page<N, Cursor, PageMeta>) {
        self.cost.record(page.meta.cost.as_ref());
        for error in page.meta.errors.drain(..) {
            self.accepted.push(AcceptedError { operation, error });
        }
    }

    fn failure(&mut self, failure: &SyncFailure) {
        if failure.decoded() {
            self.cost.record(failure.cost());
        }
    }
}

/// Fetches one connection page through the SDK adapter and applies the partial-data policy.
async fn fetch<D, N>(
    shop: &ShopContext,
    prepared: Result<Prepared<GraphqlResponse<D>>, concord::Error>,
    cursor: Option<Cursor>,
    policy: PartialData,
    project: impl FnOnce(D) -> Option<Connection<N>>,
) -> Result<Page<N, Cursor, PageMeta>, SyncFailure> {
    let page = connection_page(shop.send(prepared), cursor.as_ref(), project)
        .await
        .map_err(SyncFailure::Page)?;
    if policy == PartialData::Reject && !page.meta.errors.is_empty() {
        return Err(SyncFailure::PartialRejected {
            errors: page.meta.errors,
            cost: page.meta.cost,
        });
    }
    Ok(page)
}

/// Runs SH-A for `shop`; see the [module documentation](self).
pub async fn reconcile(shop: &ShopContext, request: &ReconcileRequest) -> ReconcileReport {
    let mut state = State::default();

    let locations = Stage::Ran(locations(shop, request, &mut state).await);
    let single_active =
        locations.exhausted() && state.locations.iter().filter(|l| l.is_active).count() == 1;
    let target = match &request.location {
        Some(id) => Some(id.clone()),
        None if single_active => state
            .locations
            .iter()
            .find(|l| l.is_active)
            .map(|l| l.id.clone()),
        None => None,
    };

    let products = if state.stop {
        Stage::NotRun(NotRun::EarlierFailure)
    } else {
        Stage::Ran(products(shop, request, &mut state).await)
    };
    let collections = if !request.collections {
        Stage::NotRun(NotRun::Disabled)
    } else if state.stop {
        Stage::NotRun(NotRun::EarlierFailure)
    } else {
        Stage::Ran(collections(shop, request, &mut state).await)
    };
    let inventory = match &target {
        _ if state.stop => Stage::NotRun(NotRun::EarlierFailure),
        None => Stage::NotRun(NotRun::NoTargetLocation),
        Some(id) => Stage::Ran(inventory(shop, request, id.clone(), &mut state).await),
    };

    let catalog_complete = products.exhausted() && state.variant_pages.complete();
    let memberships_complete = collections.exhausted() && state.member_pages.complete();
    let discrepancies = discrepancies(
        &state,
        Completeness {
            levels: inventory.exhausted(),
            single_active,
            catalog: catalog_complete && request.product_filter.is_empty(),
            memberships: memberships_complete,
        },
    );
    let all_exhausted = locations.exhausted()
        && catalog_complete
        && (!request.collections || memberships_complete)
        && inventory.exhausted();
    let status = match (all_exhausted, state.accepted.is_empty()) {
        (true, true) => ReconcileStatus::Complete,
        (true, false) => ReconcileStatus::CompleteWithErrors,
        (false, _) => ReconcileStatus::Incomplete,
    };
    let location = target.map(|id| {
        let node = state.locations.iter().find(|l| l.id == id).cloned();
        (id, node)
    });
    ReconcileReport {
        shop: shop.shop().clone(),
        location,
        locations,
        products,
        variant_pages: state.variant_pages,
        collections,
        member_pages: state.member_pages,
        inventory,
        counts: state.counts,
        discrepancies,
        accepted_errors: state.accepted,
        cost: state.cost,
        status,
    }
}

async fn locations(
    shop: &ShopContext,
    request: &ReconcileRequest,
    state: &mut State,
) -> TraversalSummary {
    let policy = request.partial_data;
    let size = request.page_sizes.locations;
    let mut pages = sequential(
        Resume::Start,
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let prepared = shop
                .admin()
                .locations(LocationsQuery::new(size).after(cursor.clone()))
                .prepare();
            fetch(shop, prepared, cursor, policy, |d| Some(d.locations))
        },
    );
    let mut failure = None;
    while let Some(result) = pages.next().await {
        match result {
            Ok(mut page) => {
                state.page("Locations", &mut page);
                state.counts.locations += page.items.len() as u64;
                state.locations.extend(page.items);
            }
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
            }
        }
    }
    TraversalSummary::new(pages.finish(), failure)
}

async fn products(
    shop: &ShopContext,
    request: &ReconcileRequest,
    state: &mut State,
) -> TraversalSummary {
    let policy = request.partial_data;
    let sizes = request.page_sizes;
    let filter = &request.product_filter;
    let mut pages = sequential(
        Resume::Start,
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let query = ProductsQuery::new(sizes.products, sizes.variants)
                .filter(filter)
                .after(cursor.clone());
            let prepared = shop.admin().products(query).prepare();
            fetch(shop, prepared, cursor, policy, |d| Some(d.products))
        },
    );
    let mut failure = None;
    loop {
        // The chain's position moves past a page when the page is emitted: keep the page's own
        // key in case a nested traversal fails while its products are processed.
        let consuming = pages.checkpoint().cloned();
        let Some(result) = pages.next().await else {
            break;
        };
        let mut page = match result {
            Ok(page) => page,
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
                continue;
            }
        };
        state.page("CatalogProducts", &mut page);
        for product in page.items {
            state.counts.products += 1;
            state
                .products
                .insert(product.id.clone(), product.status.clone());
            let (variants, more) = match product.variants.into_parts(None) {
                Ok(parts) => parts,
                Err(violation) => {
                    // The nested first page cannot be followed: record it as a failed nested
                    // traversal of this product, with no resume point.
                    state.variant_pages.incomplete.push((
                        product.id.clone(),
                        TraversalSummary {
                            outcome: Outcome::Failed,
                            pages: 0,
                            resume: None,
                            failure: Some(SyncFailure::Page(PageError::Protocol {
                                violation,
                                errors: Vec::new(),
                                cost: None,
                            })),
                        },
                    ));
                    continue;
                }
            };
            for variant in variants {
                state.variant(&product.id, variant);
            }
            if let Some(after) = more {
                nested_variants(shop, request, state, product.id, after).await;
                if state.stop {
                    break;
                }
            }
        }
        if state.stop {
            // Only a nested failure stops the run while a page is consumed.
            return TraversalSummary::interrupted(pages.finish(), consuming);
        }
    }
    TraversalSummary::new(pages.finish(), failure)
}

impl State {
    fn variant(&mut self, product: &ProductId, variant: VariantNode) {
        self.counts.variants += 1;
        self.variants.insert(
            variant.inventory_item.id,
            VariantEntry {
                product: product.clone(),
                variant: variant.id,
                sku: variant.sku,
                tracked: variant.inventory_item.tracked,
                quantity: variant.inventory_quantity,
            },
        );
    }
}

/// The rest of one product's variants, from the nested `endCursor` of its first page.
async fn nested_variants(
    shop: &ShopContext,
    request: &ReconcileRequest,
    state: &mut State,
    product: ProductId,
    after: Cursor,
) {
    if state.variant_pages.traversals as usize >= request.nested_traversals {
        state.variant_pages.skipped.push(product);
        return;
    }
    state.variant_pages.traversals += 1;
    let policy = request.partial_data;
    let size = request.page_sizes.variants;
    let id = product.clone();
    let mut pages = sequential(
        Resume::At(after),
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let query = ProductVariantsQuery::new(id.clone(), size, cursor.clone());
            let prepared = shop.admin().product_variants(query).prepare();
            fetch(shop, prepared, cursor, policy, |d| {
                d.product.map(|p| p.variants)
            })
        },
    );
    let mut failure = None;
    while let Some(result) = pages.next().await {
        match result {
            Ok(mut page) => {
                state.page("ProductVariants", &mut page);
                state.variant_pages.pages += 1;
                for variant in page.items {
                    state.variant(&product, variant);
                }
            }
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
            }
        }
    }
    let summary = TraversalSummary::new(pages.finish(), failure);
    if !summary.is_exhausted() {
        state.variant_pages.incomplete.push((product, summary));
    }
}

async fn collections(
    shop: &ShopContext,
    request: &ReconcileRequest,
    state: &mut State,
) -> TraversalSummary {
    let policy = request.partial_data;
    let sizes = request.page_sizes;
    let mut pages = sequential(
        Resume::Start,
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let query = CollectionsQuery::new(sizes.collections, sizes.collection_products)
                .after(cursor.clone());
            let prepared = shop.admin().collections(query).prepare();
            fetch(shop, prepared, cursor, policy, |d| Some(d.collections))
        },
    );
    let mut failure = None;
    loop {
        // As for products: the page's own key, in case a nested traversal fails mid-page.
        let consuming = pages.checkpoint().cloned();
        let Some(result) = pages.next().await else {
            break;
        };
        let mut page = match result {
            Ok(page) => page,
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
                continue;
            }
        };
        state.page("CatalogCollections", &mut page);
        for collection in page.items {
            state.counts.collections += 1;
            match collection.products.into_parts(None) {
                Ok((members, more)) => {
                    state.members(members.into_iter().map(|m| m.id));
                    if let Some(after) = more {
                        nested_members(shop, request, state, collection.id, after).await;
                    }
                }
                Err(violation) => state.member_pages.incomplete.push((
                    collection.id,
                    TraversalSummary {
                        outcome: Outcome::Failed,
                        pages: 0,
                        resume: None,
                        failure: Some(SyncFailure::Page(PageError::Protocol {
                            violation,
                            errors: Vec::new(),
                            cost: None,
                        })),
                    },
                )),
            }
            if state.stop {
                break;
            }
        }
        if state.stop {
            // Only a nested failure stops the run while a page is consumed.
            return TraversalSummary::interrupted(pages.finish(), consuming);
        }
    }
    TraversalSummary::new(pages.finish(), failure)
}

impl State {
    fn members(&mut self, members: impl Iterator<Item = ProductId>) {
        for member in members {
            self.counts.memberships += 1;
            self.collected.insert(member);
        }
    }
}

/// The rest of one collection's members, from the nested `endCursor` of its first page.
async fn nested_members(
    shop: &ShopContext,
    request: &ReconcileRequest,
    state: &mut State,
    collection: CollectionId,
    after: Cursor,
) {
    if state.member_pages.traversals as usize >= request.nested_traversals {
        state.member_pages.skipped.push(collection);
        return;
    }
    state.member_pages.traversals += 1;
    let policy = request.partial_data;
    let size = request.page_sizes.collection_products;
    let id = collection.clone();
    let mut pages = sequential(
        Resume::At(after),
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let query = CollectionProductsQuery::new(id.clone(), size, cursor.clone());
            let prepared = shop.admin().collection_products(query).prepare();
            fetch(shop, prepared, cursor, policy, |d| {
                d.collection.map(|c| c.products)
            })
        },
    );
    let mut failure = None;
    while let Some(result) = pages.next().await {
        match result {
            Ok(mut page) => {
                state.page("CollectionProducts", &mut page);
                state.member_pages.pages += 1;
                state.members(page.items.into_iter().map(|m| m.id));
            }
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
            }
        }
    }
    let summary = TraversalSummary::new(pages.finish(), failure);
    if !summary.is_exhausted() {
        state.member_pages.incomplete.push((collection, summary));
    }
}

async fn inventory(
    shop: &ShopContext,
    request: &ReconcileRequest,
    location: LocationId,
    state: &mut State,
) -> TraversalSummary {
    let policy = request.partial_data;
    let size = request.page_sizes.inventory_levels;
    let names = vec![QuantityName::Available, QuantityName::OnHand];
    let mut pages = sequential(
        Resume::Start,
        request.pages_per_traversal,
        |cursor: Option<Cursor>| {
            let query =
                LocationInventoryQuery::new(location.clone(), size, cursor.clone(), names.clone());
            let prepared = shop.admin().location_inventory(query).prepare();
            fetch(shop, prepared, cursor, policy, |d| {
                d.location.map(|l| l.inventory_levels)
            })
        },
    );
    let mut failure = None;
    while let Some(result) = pages.next().await {
        match result {
            Ok(mut page) => {
                let withheld = page
                    .meta
                    .errors
                    .iter()
                    .filter(|e| {
                        e.code() == Some(&concord_example_shopify_sdk::ErrorCode::AccessDenied)
                            && e.path_starts_with(&[
                                "location",
                                "inventoryLevels",
                                "nodes",
                                "item",
                                "unitCost",
                            ])
                    })
                    .count() as u64;
                state.counts.unit_costs_withheld += withheld;
                state.page("LocationInventory", &mut page);
                for level in page.items {
                    state.counts.levels += 1;
                    let available = level.quantity(QuantityName::Available);
                    state.levels.insert(
                        level.item.id,
                        LevelEntry {
                            level: level.id,
                            available,
                        },
                    );
                }
            }
            Err(e) => {
                state.failure(&e);
                state.stop = true;
                failure = Some(e);
            }
        }
    }
    TraversalSummary::new(pages.finish(), failure)
}

struct Completeness {
    levels: bool,
    single_active: bool,
    catalog: bool,
    memberships: bool,
}

fn discrepancies(state: &State, complete: Completeness) -> Vec<Discrepancy> {
    let mut found = Vec::new();
    if complete.levels {
        for (item, v) in &state.variants {
            if v.tracked && !state.levels.contains_key(item) {
                found.push(Discrepancy::TrackedNotStocked {
                    product: v.product.clone(),
                    variant: v.variant.clone(),
                    item: item.clone(),
                });
            }
        }
    }
    for (item, level) in &state.levels {
        if let Some(available) = level.available.filter(|a| *a < 0) {
            found.push(Discrepancy::NegativeAvailable {
                item: item.clone(),
                available,
            });
        }
    }
    if complete.single_active {
        for (item, v) in &state.variants {
            let location_available = state.levels.get(item).and_then(|l| l.available);
            if let (Some(variant_quantity), Some(location_available)) =
                (v.quantity, location_available)
                && variant_quantity != location_available
            {
                found.push(Discrepancy::QuantityMismatch {
                    variant: v.variant.clone(),
                    variant_quantity,
                    location_available,
                });
            }
        }
    }
    let mut by_sku: BTreeMap<&str, Vec<ProductVariantId>> = BTreeMap::new();
    let mut missing: Vec<ProductVariantId> = Vec::new();
    for v in state.variants.values() {
        match &v.sku {
            Some(sku) => by_sku.entry(sku).or_default().push(v.variant.clone()),
            None if v.tracked => missing.push(v.variant.clone()),
            None => {}
        }
    }
    missing.sort();
    found.extend(
        missing
            .into_iter()
            .map(|variant| Discrepancy::MissingSku { variant }),
    );
    for (sku, mut variants) in by_sku {
        if variants.len() > 1 {
            variants.sort();
            found.push(Discrepancy::DuplicateSku {
                sku: sku.to_owned(),
                variants,
            });
        }
    }
    if complete.catalog {
        for (item, level) in &state.levels {
            if !state.variants.contains_key(item) {
                found.push(Discrepancy::LevelWithoutVariant {
                    level: level.level.clone(),
                    item: item.clone(),
                });
            }
        }
    }
    if complete.memberships {
        for (product, status) in &state.products {
            if *status == ProductStatus::Active && !state.collected.contains(product) {
                found.push(Discrepancy::ProductInNoCollection {
                    product: product.clone(),
                });
            }
        }
    }
    found
}
