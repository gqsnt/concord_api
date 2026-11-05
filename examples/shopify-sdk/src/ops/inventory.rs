//! Inventory reads: locations, and the inventory levels (with their items and quantities) of
//! one location.

use serde::{Deserialize, Serialize};

use super::PageSize;
use crate::connection::Connection;
use crate::graphql::{Operation, OperationKind, Var, nullable};
use crate::scalars::{Cursor, DateTime, Decimal, InventoryItemId, InventoryLevelId, LocationId};

/// `locations(first:, after:, includeInactive:)`, sorted by the default `NAME`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationsQuery {
    /// Locations per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
    /// Whether deactivated locations are listed too.
    pub include_inactive: bool,
}

impl LocationsQuery {
    /// Active locations, `first` per page.
    pub fn new(first: PageSize) -> Self {
        Self {
            first,
            after: Var::Absent,
            include_inactive: false,
        }
    }

    /// Continues after `cursor` (`None`: the first page).
    #[must_use]
    pub fn after(mut self, cursor: Option<Cursor>) -> Self {
        self.after = Var::or_absent(cursor);
        self
    }
}

impl Operation for LocationsQuery {
    const NAME: &'static str = "Locations";
    const DOCUMENT: &'static str = "query Locations($first: Int!, $after: String, \
         $includeInactive: Boolean!) { \
         locations(first: $first, after: $after, includeInactive: $includeInactive) { \
         nodes { id name isActive fulfillsOnlineOrders address { city countryCode } } \
         pageInfo { hasNextPage endCursor } } }";
    const KIND: OperationKind = OperationKind::Query;
    type Data = LocationsData;
}

/// `data` of [`LocationsQuery`].
#[derive(Debug, Deserialize)]
pub struct LocationsData {
    /// `LocationConnection`.
    pub locations: Connection<LocationNode>,
}

/// A location as listed by [`LocationsQuery`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationNode {
    /// `ID!`
    pub id: LocationId,
    /// `String!`
    pub name: String,
    /// `Boolean!`
    pub is_active: bool,
    /// `Boolean!`
    pub fulfills_online_orders: bool,
    /// `LocationAddress!`
    pub address: LocationAddress,
}

/// `address { city countryCode }`: both nullable in the schema.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationAddress {
    /// `String` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub city: Option<String>,
    /// `String` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub country_code: Option<String>,
}

/// An inventory state name for `quantities(names:)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuantityName {
    /// `available`
    Available,
    /// `on_hand` (the sum of available, committed, reserved, damaged, safety stock and quality
    /// control).
    OnHand,
    /// `committed`
    Committed,
    /// `incoming`
    Incoming,
    /// `reserved`
    Reserved,
    /// `damaged`
    Damaged,
    /// `safety_stock`
    SafetyStock,
    /// `quality_control`
    QualityControl,
}

/// `location(id:) { inventoryLevels(first:, after:) }` with each level's quantities and item.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInventoryQuery {
    /// The location.
    pub id: LocationId,
    /// Levels per page.
    pub first: PageSize,
    /// The cursor to continue after.
    #[serde(skip_serializing_if = "Var::is_absent")]
    pub after: Var<Cursor>,
    /// The quantities to read (`[String!]!`).
    pub names: Vec<QuantityName>,
}

impl LocationInventoryQuery {
    /// The levels of `id` after `cursor` (`None`: from the start), reading `names`.
    pub fn new(
        id: LocationId,
        first: PageSize,
        cursor: Option<Cursor>,
        names: Vec<QuantityName>,
    ) -> Self {
        Self {
            id,
            first,
            after: Var::or_absent(cursor),
            names,
        }
    }
}

impl Operation for LocationInventoryQuery {
    const NAME: &'static str = "LocationInventory";
    const DOCUMENT: &'static str = "query LocationInventory($id: ID!, $first: Int!, \
         $after: String, $names: [String!]!) { \
         location(id: $id) { id name inventoryLevels(first: $first, after: $after) { \
         nodes { id updatedAt quantities(names: $names) { name quantity } \
         item { id sku tracked unitCost { amount currencyCode } } } \
         pageInfo { hasNextPage endCursor } } } }";
    const KIND: OperationKind = OperationKind::Query;
    type Data = LocationInventoryData;
}

/// `data` of [`LocationInventoryQuery`].
#[derive(Debug, Deserialize)]
pub struct LocationInventoryData {
    /// `Location` (nullable): `None` for an unknown id.
    #[serde(deserialize_with = "nullable")]
    pub location: Option<LocationInventory>,
}

/// The location of [`LocationInventoryQuery`].
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationInventory {
    /// `ID!`
    pub id: LocationId,
    /// `String!`
    pub name: String,
    /// `InventoryLevelConnection!`
    pub inventory_levels: Connection<InventoryLevelNode>,
}

/// An inventory level: one item stocked at one location.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryLevelNode {
    /// `ID!`
    pub id: InventoryLevelId,
    /// `DateTime!`
    pub updated_at: DateTime,
    /// `[InventoryQuantity!]!`, in the requested names.
    pub quantities: Vec<InventoryQuantity>,
    /// `InventoryItem!`
    pub item: LevelItem,
}

impl InventoryLevelNode {
    /// The quantity named `name`, if it was returned.
    pub fn quantity(&self, name: QuantityName) -> Option<i64> {
        self.quantities
            .iter()
            .find(|q| q.name == name)
            .map(|q| q.quantity)
    }
}

/// `quantities { name quantity }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct InventoryQuantity {
    /// `String!` (one of the requested names).
    pub name: QuantityName,
    /// `Int!`
    pub quantity: i64,
}

/// `item { id sku tracked unitCost { amount currencyCode } }` of a level.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelItem {
    /// `ID!`
    pub id: InventoryItemId,
    /// `String` (nullable).
    #[serde(deserialize_with = "nullable")]
    pub sku: Option<String>,
    /// `Boolean!`
    pub tracked: bool,
    /// `MoneyV2` (nullable; also `null` with a per-field `ACCESS_DENIED` error when the app may
    /// not view product costs).
    #[serde(deserialize_with = "nullable")]
    pub unit_cost: Option<MoneyV2>,
}

/// `MoneyV2 { amount currencyCode }`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyV2 {
    /// `Decimal!`
    pub amount: Decimal,
    /// `CurrencyCode!` (three letters).
    pub currency_code: String,
}
