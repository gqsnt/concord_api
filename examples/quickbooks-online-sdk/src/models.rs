//! Identifiers, entities and request bodies of the slice. Field names follow the Customer and
//! Invoice API reference pages (PROVENANCE.md, D2 and D3); unknown fields are ignored.

use std::collections::hash_map::RandomState;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::value::RawValue;

use crate::Error;

/// The longest identifier this SDK accepts (realm and entity IDs are decimal numbers).
const MAX_ID_LEN: usize = 32;

fn decimal_digits(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_LEN && value.bytes().all(|b| b.is_ascii_digit())
}

/// A QuickBooks Online company ("realm") ID: ASCII digits, as in the `realmId` callback parameter
/// and every Accounting URL (`/v3/company/{realmId}/...`).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RealmId(String);

impl RealmId {
    /// Validates a realm ID: 1 to 32 ASCII digits.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] otherwise; nothing is sent.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if decimal_digits(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput { what: "realm ID" })
        }
    }

    /// The ID as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RealmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RealmId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RealmId({})", self.0)
    }
}

/// The ID of one entity (a customer, an invoice) within a realm: ASCII digits, sent as one path
/// segment. QuickBooks documents IDs as strings that must be valid numbers (error 2030).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntityId(String);

impl EntityId {
    /// Validates an entity ID: 1 to 32 ASCII digits.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] otherwise; nothing is sent.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if decimal_digits(&value) {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput { what: "entity ID" })
        }
    }

    /// The ID as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(|_| D::Error::custom("an entity Id that is not 1 to 32 digits"))
    }
}

impl Serialize for EntityId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// The `requestid` query parameter of a write: QuickBooks answers a repeated request with the same
/// ID with the original response instead of performing it again. Unique per realm, at most 50
/// characters; this SDK accepts ASCII letters, digits, `-` and `_`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// The longest request ID QuickBooks accepts outside batch operations.
    pub const MAX_LEN: usize = 50;

    /// Validates a caller-chosen request ID (a UUID, an order number).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when empty, longer than [`MAX_LEN`](Self::MAX_LEN) or using other
    /// characters; nothing is sent.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= Self::MAX_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if valid {
            Ok(Self(value))
        } else {
            Err(Error::InvalidInput { what: "request ID" })
        }
    }

    /// A fresh 32-character hexadecimal ID: 128 bits from the standard library's per-process
    /// random hasher keys, a process-wide counter and the clock. Collisions are improbable, not
    /// impossible; supply your own with [`RequestId::new`] when your system already has a unique
    /// key for the write.
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let keys = RandomState::new();
        let mut high = keys.build_hasher();
        high.write_u64(count);
        high.write_u128(nanos);
        high.write_u32(std::process::id());
        let high = high.finish();
        let mut low = keys.build_hasher();
        low.write_u64(high);
        low.write_u64(!count);
        Self(format!("{high:016x}{:016x}", low.finish()))
    }

    /// The ID as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RequestId({})", self.0)
    }
}

/// A decimal number kept exactly as QuickBooks writes it (`100.0`, `362.07`, `9.5`): amounts,
/// balances, quantities and unit prices are never converted through `f64`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Decimal(String);

impl Decimal {
    /// Parses `-?digits[.digits]` (no exponent, no `+`, no spaces).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for anything else.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let digits = text.strip_prefix('-').unwrap_or(text);
        let (whole, fraction) = match digits.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (digits, None),
        };
        let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        if all_digits(whole) && fraction.is_none_or(all_digits) && text.len() <= 40 {
            Ok(Self(text.to_owned()))
        } else {
            Err(Error::InvalidInput {
                what: "decimal number",
            })
        }
    }

    /// An amount in hundredths (`10050` is `100.50`).
    pub fn from_cents(cents: i64) -> Self {
        let sign = if cents < 0 { "-" } else { "" };
        let magnitude = cents.unsigned_abs();
        Self(format!("{sign}{}.{:02}", magnitude / 100, magnitude % 100))
    }

    /// The number as text, exactly as received or constructed.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Decimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Decimal({})", self.0)
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Self::parse(raw.get()).map_err(|_| D::Error::custom("a number that is not a plain decimal"))
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RawValue::from_string(self.0.clone())
            .map_err(serde::ser::Error::custom)?
            .serialize(serializer)
    }
}

/// A QuickBooks `ReferenceType`: the `value` (an ID or a code such as `TAX`) and optional `name`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Reference {
    /// The referenced object's ID or code.
    pub value: String,
    /// Its display name, when QuickBooks includes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// An email address element (`{"Address": ..}`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EmailAddress {
    /// The address.
    pub address: Option<String>,
}

/// `MetaData`: creation and last-update timestamps, as QuickBooks writes them.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MetaData {
    /// `CreateTime`.
    pub create_time: Option<String>,
    /// `LastUpdatedTime`.
    pub last_updated_time: Option<String>,
}

/// A customer (selected fields of the Customer object).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Customer {
    /// `Id`, unique within the realm.
    pub id: EntityId,
    /// `SyncToken`, the object's version.
    pub sync_token: String,
    /// `DisplayName`, unique across customers, vendors and employees of the realm.
    pub display_name: Option<String>,
    /// `CompanyName`.
    pub company_name: Option<String>,
    /// `GivenName`.
    pub given_name: Option<String>,
    /// `FamilyName`.
    pub family_name: Option<String>,
    /// `Active`.
    pub active: Option<bool>,
    /// `Balance`: the open balance.
    pub balance: Option<Decimal>,
    /// `PrimaryEmailAddr`.
    pub primary_email_addr: Option<EmailAddress>,
    /// `CurrencyRef`.
    pub currency_ref: Option<Reference>,
    /// `MetaData`.
    pub meta_data: Option<MetaData>,
}

/// The Read a customer response: `{"Customer": {..}, "time": ..}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CustomerResponse {
    /// The customer.
    #[serde(rename = "Customer")]
    pub customer: Customer,
    /// The server's timestamp.
    pub time: Option<String>,
}

/// One page of a customer query: `QueryResponse`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CustomerPage {
    /// The customers; QuickBooks omits the member when nothing matched.
    #[serde(rename = "Customer", default)]
    pub customers: Vec<Customer>,
    /// `startPosition`, one-based.
    #[serde(rename = "startPosition")]
    pub start_position: Option<u32>,
    /// `maxResults`, as QuickBooks reports it.
    #[serde(rename = "maxResults")]
    pub max_results: Option<u32>,
}

/// The Query a customer response: `{"QueryResponse": {..}, "time": ..}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct CustomerQueryResponse {
    /// The page.
    #[serde(rename = "QueryResponse")]
    pub page: CustomerPage,
    /// The server's timestamp.
    pub time: Option<String>,
}

/// `SalesItemLineDetail` of an invoice line.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SalesItemLineDetail {
    /// `ItemRef`: the product or service.
    pub item_ref: Option<Reference>,
    /// `Qty`.
    pub qty: Option<Decimal>,
    /// `UnitPrice`.
    pub unit_price: Option<Decimal>,
    /// `TaxCodeRef`.
    pub tax_code_ref: Option<Reference>,
}

/// One invoice line as QuickBooks returns it (sales item lines and the subtotal line).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Line {
    /// `Id`, absent on the computed subtotal line.
    pub id: Option<String>,
    /// `LineNum`.
    pub line_num: Option<u32>,
    /// `Amount`.
    pub amount: Option<Decimal>,
    /// `DetailType` (`SalesItemLineDetail`, `SubTotalLineDetail`, ...).
    pub detail_type: String,
    /// `Description`.
    pub description: Option<String>,
    /// The sales item detail, for a sales item line.
    pub sales_item_line_detail: Option<SalesItemLineDetail>,
}

/// An invoice (selected fields of the Invoice object).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Invoice {
    /// `Id`, unique within the realm.
    pub id: EntityId,
    /// `SyncToken`, the object's version.
    pub sync_token: String,
    /// `DocNumber`, assigned by QuickBooks when not supplied.
    pub doc_number: Option<String>,
    /// `TxnDate`.
    pub txn_date: Option<String>,
    /// `DueDate`.
    pub due_date: Option<String>,
    /// `CustomerRef`: the invoiced customer.
    pub customer_ref: Reference,
    /// `Line`.
    #[serde(default)]
    pub line: Vec<Line>,
    /// `TotalAmt`.
    pub total_amt: Option<Decimal>,
    /// `Balance`.
    pub balance: Option<Decimal>,
    /// `MetaData`.
    pub meta_data: Option<MetaData>,
}

/// The Create and Read an invoice response: `{"Invoice": {..}, "time": ..}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct InvoiceResponse {
    /// The invoice.
    #[serde(rename = "Invoice")]
    pub invoice: Invoice,
    /// The server's timestamp.
    pub time: Option<String>,
}

/// A sales item line for a new invoice: `DetailType: SalesItemLineDetail`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SalesItemLine {
    amount: Decimal,
    item: EntityId,
    description: Option<String>,
    quantity: Option<Decimal>,
    unit_price: Option<Decimal>,
}

impl SalesItemLine {
    /// A line of `amount` for the product or service `item` (its `ItemRef.value`).
    pub fn new(amount: Decimal, item: EntityId) -> Self {
        Self {
            amount,
            item,
            description: None,
            quantity: None,
            unit_price: None,
        }
    }

    /// `Description`, sent as given (QuickBooks validates it).
    #[must_use]
    pub fn description(mut self, text: impl Into<String>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// `Qty` and `UnitPrice`. QuickBooks checks that `Amount` equals their product (error 6070).
    #[must_use]
    pub fn quantity(mut self, quantity: Decimal, unit_price: Decimal) -> Self {
        self.quantity = Some(quantity);
        self.unit_price = Some(unit_price);
        self
    }
}

impl Serialize for SalesItemLine {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Detail<'a> {
            item_ref: ItemRef<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            qty: Option<&'a Decimal>,
            #[serde(skip_serializing_if = "Option::is_none")]
            unit_price: Option<&'a Decimal>,
        }
        #[derive(Serialize)]
        struct ItemRef<'a> {
            value: &'a EntityId,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Wire<'a> {
            detail_type: &'static str,
            amount: &'a Decimal,
            #[serde(skip_serializing_if = "Option::is_none")]
            description: Option<&'a str>,
            sales_item_line_detail: Detail<'a>,
        }
        Wire {
            detail_type: "SalesItemLineDetail",
            amount: &self.amount,
            description: self.description.as_deref(),
            sales_item_line_detail: Detail {
                item_ref: ItemRef { value: &self.item },
                qty: self.quantity.as_ref(),
                unit_price: self.unit_price.as_ref(),
            },
        }
        .serialize(serializer)
    }
}

/// The body of Create an invoice: a `CustomerRef` and at least one line (both required by
/// QuickBooks). Built with [`NewInvoice::new`], so an invoice without a line cannot be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewInvoice {
    customer: EntityId,
    lines: Vec<SalesItemLine>,
    doc_number: Option<String>,
}

impl NewInvoice {
    /// The longest `DocNumber` QuickBooks accepts.
    pub const DOC_NUMBER_MAX: usize = 21;

    /// An invoice for `customer` (its `CustomerRef.value`) with its first line.
    pub fn new(customer: EntityId, first_line: SalesItemLine) -> Self {
        Self {
            customer,
            lines: vec![first_line],
            doc_number: None,
        }
    }

    /// Adds a line.
    #[must_use]
    pub fn line(mut self, line: SalesItemLine) -> Self {
        self.lines.push(line);
        self
    }

    /// `DocNumber`: 1 to 21 characters. QuickBooks assigns the next number when it is absent; it
    /// is not an idempotency key (the request ID is).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] otherwise; nothing is sent.
    pub fn doc_number(mut self, number: &str) -> Result<Self, Error> {
        let count = number.chars().count();
        if count == 0 || count > Self::DOC_NUMBER_MAX {
            return Err(Error::InvalidInput { what: "DocNumber" });
        }
        self.doc_number = Some(number.to_owned());
        Ok(self)
    }

    /// The customer the invoice is for.
    pub fn customer(&self) -> &EntityId {
        &self.customer
    }
}

impl Serialize for NewInvoice {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct CustomerRef<'a> {
            value: &'a EntityId,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Wire<'a> {
            line: &'a [SalesItemLine],
            customer_ref: CustomerRef<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            doc_number: Option<&'a str>,
        }
        Wire {
            line: &self.lines,
            customer_ref: CustomerRef {
                value: &self.customer,
            },
            doc_number: self.doc_number.as_deref(),
        }
        .serialize(serializer)
    }
}
