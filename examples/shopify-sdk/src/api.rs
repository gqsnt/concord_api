//! The declaration: one GraphQL endpoint per shop and API version, one typed endpoint per named
//! operation.

use concord::Json;
use concord::reqwest::header::HeaderName;

use crate::graphql::{Answer, GraphQl};
use crate::ops::bulk::{BulkOperationQuery, CancelBulkOperation, RunBulkQuery};
use crate::ops::catalog::{
    CollectionProductsQuery, CollectionsQuery, ProductQuery, ProductVariantsQuery, ProductsQuery,
};
use crate::ops::inventory::{LocationInventoryQuery, LocationsQuery};
use crate::ops::shop::ShopQuery;
use crate::scalars::{ApiVersion, ShopDomain};

/// `X-Shopify-Access-Token`: the Admin API credential. Not on Concord's list of well-known secret
/// header names, so the value placed under it is declared `#[sensitive]`.
pub const X_SHOPIFY_ACCESS_TOKEN: HeaderName = HeaderName::from_static("x-shopify-access-token");

concord::api! {
    /// The Shopify GraphQL Admin API (a bounded slice, see `PROVIDER.md`).
    ///
    /// Every operation is `POST https://{shop}.myshopify.com/admin/api/{version}/graphql.json`
    /// with a JSON body (`GraphQl`: `query`, `operationName`, `variables`) and a JSON envelope
    /// (`Answer<Op>`, the `GraphqlResponse` of the operation's own `Data`) that keeps `errors`
    /// and `extensions.cost` beside `data`. Each endpoint names its operation twice, as the body
    /// type and in `Answer<..>`, so the output type is derived from the operation, not restated.
    pub client ShopifyAdmin(
        /// The API version in the path; pinned to `2026-10` unless replaced.
        version: ApiVersion = ApiVersion::PINNED,
    ) {
        base "https://myshopify.com";
        request GraphQl;
        response Json;

        /// One shop and its access token. Tokens are per shop (per app installation), so the
        /// token belongs to this scope, not to the client.
        scope shop(
            shop: ShopDomain,
            #[sensitive] header(X_SHOPIFY_ACCESS_TOKEN) access_token: String,
        ) {
            subdomain "{shop}";
            path "admin/api/{client.version}/graphql.json";

            /// `ShopIdentity`: which shop answered.
            POST shop_info(body variables: ShopQuery) -> Answer<ShopQuery>;
            /// `CatalogProducts`: a page of products with their first variants.
            POST products(body variables: ProductsQuery) -> Answer<ProductsQuery>;
            /// `CatalogProduct`: one product by id.
            POST product(body variables: ProductQuery) -> Answer<ProductQuery>;
            /// `ProductVariants`: a page of one product's variants.
            POST product_variants(body variables: ProductVariantsQuery)
                -> Answer<ProductVariantsQuery>;
            /// `CatalogCollections`: a page of collections with their first member products.
            POST collections(body variables: CollectionsQuery) -> Answer<CollectionsQuery>;
            /// `CollectionProducts`: a page of one collection's member products.
            POST collection_products(body variables: CollectionProductsQuery)
                -> Answer<CollectionProductsQuery>;
            /// `Locations`: a page of locations.
            POST locations(body variables: LocationsQuery) -> Answer<LocationsQuery>;
            /// `LocationInventory`: a page of one location's inventory levels.
            POST location_inventory(body variables: LocationInventoryQuery)
                -> Answer<LocationInventoryQuery>;
            /// `BulkRunQuery` (**mutation**: creates a bulk job on the shop).
            POST run_bulk_query(body variables: RunBulkQuery) -> Answer<RunBulkQuery>;
            /// `BulkOperationStatus`: one bulk operation by id.
            POST bulk_operation(body variables: BulkOperationQuery) -> Answer<BulkOperationQuery>;
            /// `BulkOperationCancel` (**mutation**: starts canceling a bulk job).
            POST cancel_bulk_operation(body variables: CancelBulkOperation)
                -> Answer<CancelBulkOperation>;
        }
    }
}
