//! S2: `https://{region}.api.example.com/v2/orgs/{org}/catalog/...` with a sensitive API key, a
//! shared `Accept` and an `X-Api-Version` header that has a default, can be changed on the client
//! and overridden per call.

use concord::Json;
use std::fmt;

use serde::Deserialize;

/// The default `X-Api-Version`.
pub const DEFAULT_API_VERSION: &str = "2024-06-01";

/// Deployment region; its `Display` is the DNS label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Europe.
    Eu,
    /// North America.
    Na,
    /// Asia-Pacific.
    Ap,
}

impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Region::Eu => "eu",
            Region::Na => "na",
            Region::Ap => "ap",
        })
    }
}

/// Regional status.
#[derive(Debug, Deserialize)]
pub struct Status {
    /// Whether the region is healthy.
    pub healthy: bool,
}

/// A catalog product.
#[derive(Debug, Deserialize)]
pub struct Product {
    /// Identifier.
    pub id: String,
    /// Title.
    pub title: String,
}

concord::api! {
    /// The regional catalog API.
    pub client CatalogApi(
        /// The API key; sent on every request, never shown by Concord.
        #[sensitive] header("x-api-key") api_key: String,
        /// The API version sent unless an endpoint call overrides it.
        header("x-api-version") api_version: String = DEFAULT_API_VERSION,
    ) {
        base "https://api.example.com/v2";
        headers { "accept" = "application/json"; }
        response Json;

        scope regional(region: Region) {
            subdomain "{region}";

            GET status() -> Status { path "status"; }

            scope org(org: String) {
                path "orgs/{org}/catalog";

                GET products(version?: String) -> Vec<Product> {
                    path "products";
                    headers { override "x-api-version" = version else inherit; }
                }
                GET product(id: String, version?: String) -> Product {
                    path "products/{id}";
                    headers { override "x-api-version" = version else inherit; }
                }
            }
        }
    }
}
