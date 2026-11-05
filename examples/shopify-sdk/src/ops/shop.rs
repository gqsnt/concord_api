//! The shop itself: a cheap read that identifies which shop answered.

use serde::{Deserialize, Serialize};

use crate::graphql::{Operation, OperationKind};
use crate::scalars::ShopId;

/// `shop { id name myshopifyDomain currencyCode }` (no variables: sent as `{}`).
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct ShopQuery {}

impl Operation for ShopQuery {
    const NAME: &'static str = "ShopIdentity";
    const DOCUMENT: &'static str =
        "query ShopIdentity { shop { id name myshopifyDomain currencyCode } }";
    const KIND: OperationKind = OperationKind::Query;
    type Data = ShopData;
}

/// `data` of [`ShopQuery`].
#[derive(Debug, Deserialize)]
pub struct ShopData {
    /// `Shop!`
    pub shop: ShopNode,
}

/// The shop as selected by [`ShopQuery`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopNode {
    /// `ID!`
    pub id: ShopId,
    /// `String!`
    pub name: String,
    /// `String!`, `acme.myshopify.com`.
    pub myshopify_domain: String,
    /// `CurrencyCode!`
    pub currency_code: String,
}
