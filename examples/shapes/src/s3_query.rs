//! S3: a search endpoint with eight optional query parameters (one repeated, one defaulted), a JSON
//! `POST` and a `DELETE` that answers 204, all with a bearer token.

use std::fmt;

use concord::Json;
use serde::{Deserialize, Serialize};

/// Sort order; its `Display` is the wire spelling.
#[derive(Clone, Copy, Debug)]
pub enum Sort {
    /// Best match first.
    Relevance,
    /// Cheapest first.
    PriceAsc,
    /// Most expensive first.
    PriceDesc,
    /// Newest first.
    Newest,
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Relevance => "relevance",
            Sort::PriceAsc => "price_asc",
            Sort::PriceDesc => "price_desc",
            Sort::Newest => "newest",
        })
    }
}

/// An item.
#[derive(Debug, Deserialize)]
pub struct Item {
    /// Identifier.
    pub id: String,
    /// Name.
    pub name: String,
    /// Price in cents.
    pub price_cents: u32,
}

/// One page of search results.
#[derive(Debug, Deserialize)]
pub struct SearchPage {
    /// Items on this page.
    pub items: Vec<Item>,
    /// Cursor for the next page, if any.
    pub next_cursor: Option<String>,
}

/// Input for `create_item`.
#[derive(Debug, Serialize)]
pub struct NewItem {
    /// Name.
    pub name: String,
    /// Price in cents.
    pub price_cents: u32,
    /// Tags.
    pub tags: Vec<String>,
}

concord::api! {
    /// The shop API.
    pub client ShopApi(#[sensitive] token: String) {
        base "https://api.example.com/v1";
        headers { "authorization" = "Bearer {token}"; }

        GET search(
            query q?: String,
            query("tag") each tags?: Vec<String>,
            query category?: String,
            query min_price?: u32,
            query max_price?: u32,
            query in_stock?: bool,
            query sort?: Sort,
            query cursor?: String,
            query limit: u32 = 20,
        ) -> Json<SearchPage> {
            path "items/search";
        }

        POST create_item(body item: Json<NewItem>) -> Json<Item> { path "items"; }

        DELETE delete_item(id: String) { path "items/{id}"; }
    }
}
