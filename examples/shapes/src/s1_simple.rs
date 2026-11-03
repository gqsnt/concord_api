//! S1: a base URL, `GET /health` and `GET /items/{id}`, both JSON.

use concord::Json;
use serde::Deserialize;

/// `GET /health` response.
#[derive(Debug, Deserialize)]
pub struct Health {
    /// Service status.
    pub status: String,
}

/// An item.
#[derive(Debug, Deserialize)]
pub struct Item {
    /// Identifier.
    pub id: String,
    /// Display name.
    pub name: String,
}

concord::api! {
    /// The simple API.
    pub client SimpleApi {
        base "https://api.example.com/v1";
        response Json;

        GET health() -> Health { path "health"; }
        GET item(id: String) -> Item { path "items/{id}"; }
    }
}
