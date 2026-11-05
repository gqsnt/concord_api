//! A bounded slice of the Shopify GraphQL Admin API, version `2026-10`, declared with
//! `concord::api!`. It is not a complete Shopify SDK: `PROVIDER.md` lists what is supported, what
//! is deliberately not, and the sources (retrieved 4 November 2026).
//!
//! | Module | What it holds |
//! |---|---|
//! | [`api`] | the declaration: one `POST graphql.json` endpoint per named operation, under a `shop` scope carrying the shop subdomain and its `#[sensitive]` token |
//! | [`ops`] | the operations: typed variables (the request body) and typed `data` for shop, catalog, inventory and bulk documents |
//! | [`graphql`] | the [`GraphQl`] request format, the [`GraphqlResponse`] envelope (errors and cost kept beside data), [`Var`] (absent / `null` / value), [`Included`] (a field selected on request) |
//! | [`connection`] | [`Connection`] and the [`connection_page`] adapter for `concord-pagination` |
//! | [`context`] | [`ShopifyContext`] / [`ShopContext`]: configuration, the version-checked [`ShopContext::send`], the credential-free result download |
//! | [`bulk_export`] | the catalog export selection, its typed JSONL lines ([`BulkRecord`]; [`CatalogLine`] when metafields are selected) and the configured [`result_reader`](bulk_export::result_reader) |
//! | [`scalars`] | ids, shop, token, cursor, signed URL, `Decimal`, `DateTime`, `UnsignedInt64` |
//! | [`search`] | Shopify search syntax for `query:` filters |
//!
//! ```no_run
//! use concord::reqwest;
//! use concord_example_shopify_sdk::ops::catalog::ProductsQuery;
//! use concord_example_shopify_sdk::ops::PageSize;
//! use concord_example_shopify_sdk::{AccessToken, ShopDomain, ShopifyContext};
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let http = reqwest::Client::builder()
//!     .redirect(reqwest::redirect::Policy::none())
//!     .build()?;
//! let sdk = ShopifyContext::new(http);
//! let shop = sdk.shop(ShopDomain::new("acme")?, &AccessToken::new("shpat_example")?);
//!
//! let query = ProductsQuery::new(PageSize::new(50)?, PageSize::new(10)?);
//! let answer = shop.send(shop.admin().products(query).prepare()).await?;
//! if let Some(cost) = answer.cost() {
//!     println!("cost {:?}, {} error(s)", cost.actual_query_cost, answer.errors.len());
//! }
//! # Ok(()) }
//! ```

pub mod api;
pub mod bulk_export;
pub mod connection;
pub mod context;
pub mod graphql;
pub mod ops;
pub mod scalars;
pub mod search;

pub use api::{ShopifyAdmin, ShopifyAdminShop, X_SHOPIFY_ACCESS_TOKEN};
pub use bulk_export::{BulkRecord, CatalogExport, CatalogLine};
pub use connection::{Connection, PageError, PageInfo, PageMeta, connection_page};
pub use context::{RequestError, ShopContext, ShopifyContext};
pub use graphql::{
    Answer, Data, ErrorCode, GraphQl, GraphqlError, GraphqlResponse, Included, Operation,
    QueryCost, ThrottleStatus, Var,
};
pub use scalars::{AccessToken, ApiVersion, Cursor, ShopDomain, SignedUrl};
pub use search::SearchQuery;
