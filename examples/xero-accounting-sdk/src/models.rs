//! Tenant-bound resources and the deliberately small invoice representation.
use crate::{Error, ErrorKind};
use serde::{Deserialize, Serialize, Serializer};
use std::fmt;

pub(crate) fn uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// A tenant ID selected from the application's authorized Xero connections.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TenantId(String);
impl TenantId {
    /// Validate a UUID-shaped identifier. Authorization is supplied by the application grant.
    pub fn new(value: impl Into<String>) -> Result<Self, Error> {
        let value = value.into();
        if !uuid(&value) {
            return Err(Error::input("tenant ID must be a UUID"));
        }
        Ok(Self(value.to_ascii_lowercase()))
    }
    /// Explicit access to the wire identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TenantId(<withheld>)")
    }
}

/// An invoice identifier permanently bound to the tenant that produced it.
#[derive(Clone, PartialEq, Eq)]
pub struct InvoiceRef {
    pub(crate) tenant: TenantId,
    pub(crate) id: String,
}
impl InvoiceRef {
    /// Explicit access to Xero's identifier.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// The owning tenant.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }
}
impl fmt::Debug for InvoiceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InvoiceRef(<withheld>)")
    }
}

/// A pre-existing contact chosen within one tenant (no Contacts endpoint is implemented).
#[derive(Clone)]
pub struct ContactRef {
    pub(crate) tenant: TenantId,
    pub(crate) id: String,
}
impl fmt::Debug for ContactRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContactRef(<withheld>)")
    }
}

/// An exact nonnegative amount with two decimal places, serialized as a JSON number.
#[derive(Clone, Copy, Debug)]
pub struct Amount(u64);
impl Amount {
    /// Construct from minor units, up to 999,999,999.99. No float arithmetic is performed.
    pub fn from_minor_units(value: u64) -> Result<Self, Error> {
        if value > 99_999_999_999 {
            return Err(Error::input("amount exceeds this slice's bound"));
        }
        Ok(Self(value))
    }
}
impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let text = format!("{}.{:02}", self.0 / 100, self.0 % 100);
        let raw =
            serde_json::value::RawValue::from_string(text).map_err(serde::ser::Error::custom)?;
        raw.serialize(s)
    }
}

/// One draft invoice line; this slice leaves tax/account defaults to Xero.
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LineItem {
    description: String,
    quantity: u32,
    unit_amount: Amount,
}
impl LineItem {
    /// A description, positive integral quantity and exact unit amount.
    pub fn new(
        description: impl Into<String>,
        quantity: u32,
        unit_amount: Amount,
    ) -> Result<Self, Error> {
        let description = description.into();
        if description.trim().is_empty() || description.len() > 4000 || quantity == 0 {
            return Err(Error::input(
                "line needs a description and positive quantity within slice bounds",
            ));
        }
        Ok(Self {
            description,
            quantity,
            unit_amount,
        })
    }
}
impl fmt::Debug for LineItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LineItem(<withheld>)")
    }
}

/// A single sales invoice whose Status is always DRAFT and Type always ACCREC.
#[derive(Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DraftInvoice {
    #[serde(skip)]
    pub(crate) tenant: TenantId,
    #[serde(rename = "Type")]
    invoice_type: &'static str,
    status: &'static str,
    contact: ContactBody,
    line_items: Vec<LineItem>,
}
#[derive(Clone, Serialize)]
struct ContactBody {
    #[serde(rename = "ContactID")]
    id: String,
}
impl DraftInvoice {
    pub(crate) fn new(contact: ContactRef, lines: Vec<LineItem>) -> Result<Self, Error> {
        if lines.is_empty() || lines.len() > 100 {
            return Err(Error::input("draft needs 1 to 100 lines"));
        }
        Ok(Self {
            tenant: contact.tenant,
            invoice_type: "ACCREC",
            status: "DRAFT",
            contact: ContactBody { id: contact.id },
            line_items: lines,
        })
    }
}
impl fmt::Debug for DraftInvoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DraftInvoice(<withheld>)")
    }
}

/// Returned invoice state. Unmodeled fields, tax calculations and state transitions stay in Xero.
#[derive(Clone)]
pub struct Invoice {
    /// Tenant-bound reference usable for a later retrieval.
    pub reference: InvoiceRef,
    /// Status string, preserving future provider statuses.
    pub status: String,
    /// Invoice type (this slice creates ACCREC only).
    pub invoice_type: String,
    /// Human-facing invoice number, if returned.
    pub invoice_number: Option<String>,
}
impl fmt::Debug for Invoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Invoice(<withheld>)")
    }
}

#[derive(Deserialize)]
pub(crate) struct WireInvoice {
    #[serde(rename = "InvoiceID")]
    id: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
    #[serde(rename = "Type")]
    invoice_type: Option<String>,
    #[serde(rename = "InvoiceNumber")]
    invoice_number: Option<String>,
    #[serde(rename = "HasValidationErrors", default)]
    invalid: bool,
    #[serde(rename = "ValidationErrors", default)]
    errors: Vec<serde_json::Value>,
    #[serde(rename = "StatusAttributeString")]
    result: Option<String>,
}
impl WireInvoice {
    pub(crate) fn bind(self, tenant: &TenantId) -> Result<Invoice, Error> {
        if self.invalid || !self.errors.is_empty() || self.result.as_deref() == Some("ERROR") {
            return Err(Error::new(ErrorKind::InvoiceValidation));
        }
        let id = self
            .id
            .filter(|s| uuid(s))
            .ok_or_else(|| Error::new(ErrorKind::Contract("missing or invalid invoice ID")))?;
        let status = self
            .status
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::new(ErrorKind::Contract("missing invoice status")))?;
        let invoice_type = self
            .invoice_type
            .filter(|s| !s.is_empty())
            .ok_or_else(|| Error::new(ErrorKind::Contract("missing invoice type")))?;
        Ok(Invoice {
            reference: InvoiceRef {
                tenant: tenant.clone(),
                id: id.to_ascii_lowercase(),
            },
            status,
            invoice_type,
            invoice_number: self.invoice_number,
        })
    }
}

/// Optional modern paging metadata, validated against each request before traversal.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    /// One-based current page.
    pub page: u32,
    /// Effective page size.
    pub page_size: u32,
    /// Number of pages (zero is accepted only for an empty first page).
    pub page_count: u32,
    /// Total records.
    pub item_count: u64,
}
#[derive(Deserialize)]
pub(crate) struct InvoicesDocument {
    #[serde(rename = "Invoices")]
    pub invoices: Vec<WireInvoice>,
    #[serde(alias = "Pagination")]
    pub pagination: Option<Pagination>,
    #[serde(rename = "Status")]
    pub status: Option<String>,
}
#[derive(Serialize)]
pub(crate) struct CreateBody {
    #[serde(rename = "Invoices")]
    pub invoices: Vec<DraftInvoice>,
}
