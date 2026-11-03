//! The one Concord declaration every recipe uses: a small inventory API.

use concord::{Binary, Json};
use serde::{Deserialize, Serialize};

use crate::repr::{self, Ndjson};

/// An inventory item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Identifier.
    pub id: u64,
    /// Display name.
    pub name: String,
}

/// Body of `PUT items/{id}`.
#[derive(Debug, Clone, Serialize)]
pub struct Rename {
    /// The new name.
    pub name: String,
}

/// One page of `GET items`.
#[derive(Debug, Clone, Deserialize)]
pub struct Page {
    /// Items on this page.
    pub items: Vec<Item>,
    /// Opaque cursor of the next page; `None` on the last page.
    pub next: Option<String>,
}

/// One stock row, exchanged as NDJSON (one JSON object per line).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockRow {
    /// Stock-keeping unit.
    pub sku: String,
    /// Quantity on hand.
    pub qty: u32,
}

concord::api! {
    /// A small inventory API. `key` is the full `Authorization` value (e.g. `Bearer ...`).
    pub client Inventory(#[sensitive] header("authorization") key: String) {
        base "http://inventory.invalid/v1";
        request Json;
        response Json;

        /// One item.
        GET item(id: u64) -> Item { path "items/{id}"; }

        /// Renames an item; a failure is answered with an RFC 9457 problem document.
        PUT rename(id: u64, body change: Rename) -> Item { path "items/{id}"; }

        /// A cursor-paginated listing.
        GET list(query limit: u32 = 2, query cursor?: String) -> Page { path "items"; }

        /// Search with an application-built filter sequence (see `query_edit`).
        GET search() -> Vec<Item> {
            path "search";
            query { "limit" = "10"; }
        }

        /// Bulk stock import and its per-row result, both NDJSON.
        POST import_stock(body rows: Vec<StockRow>) -> Vec<StockRow> {
            format Ndjson;
            path "stock/import";
        }

        /// A form-encoded feedback submission, with this crate's pair encoder.
        POST feedback(body fields: Vec<(String, String)>) {
            request repr::Form;
            path "feedback";
        }

        /// A long chunked event export. The declared `bytes` buffers everything; the streaming
        /// recipe uses `send_raw` instead.
        GET events() -> concord::bytes::Bytes { response Binary; path "events"; }
    }
}
