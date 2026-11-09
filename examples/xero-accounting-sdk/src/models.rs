//! Tenant-bound resources and the deliberately small invoice representation.
use crate::{Error, ErrorKind};
use serde::{Deserialize, Serialize, Serializer};
use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize, concord::Debug)]
#[serde(transparent)]
pub struct TenantId(#[sensitive] String);
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

/// An invoice identifier permanently bound to the tenant that produced it.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
#[sensitive]
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

/// Whole UTC seconds since the Unix epoch, inside the four-digit-year range of Xero's
/// documented `If-Modified-Since` spelling `yyyy-mm-ddThh:mm:ss`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Utc(u64);
/// 9999-12-31T23:59:59 UTC.
const LAST_UTC_SECOND: u64 = 253_402_300_799;
impl Utc {
    /// Truncate to the second: Xero filters "accurate to the second", and rounding down can
    /// only repeat an invoice in the next sync, never skip one.
    fn new(at: SystemTime) -> Result<Self, Error> {
        let seconds = at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::input("modified-since precedes 1970-01-01T00:00:00 UTC"))?
            .as_secs();
        if seconds > LAST_UTC_SECOND {
            return Err(Error::input(
                "modified-since is after 9999-12-31T23:59:59 UTC",
            ));
        }
        Ok(Self(seconds))
    }
}
/// The wire text, computed from the instant (never from a local clock reading or zone).
impl fmt::Display for Utc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Proleptic Gregorian civil date from days since 1970-01-01 (Hinnant's
        // `civil_from_days`, restricted to nonnegative days so unsigned arithmetic is exact).
        let (days, second) = (self.0 / 86_400, self.0 % 86_400);
        let z = days + 719_468;
        let (era, doe) = (z / 146_097, z % 146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = era * 400 + yoe + u64::from(month <= 2);
        write!(
            f,
            "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
            second / 3600,
            second / 60 % 60,
            second % 60
        )
    }
}
impl fmt::Debug for Utc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// A tenant's incremental-sync point: list invoices created or modified since this UTC second.
/// Xero does not document whether the bound is inclusive; expect overlap and upsert by ID.
///
/// Like every tenant-bound value it is refused by another tenant before I/O, so one tenant's
/// sync progress can never filter (and silently truncate) another tenant's listing.
#[derive(Clone, PartialEq, Eq)]
pub struct ModifiedSince {
    pub(crate) tenant: TenantId,
    pub(crate) at: Utc,
}
impl ModifiedSince {
    pub(crate) fn new(tenant: TenantId, at: SystemTime) -> Result<Self, Error> {
        Ok(Self {
            tenant,
            at: Utc::new(at)?,
        })
    }
    /// The instant sent, already truncated to the whole second. Persist it (for example as
    /// seconds since the Unix epoch) and rebind it with `Tenant::modified_since` after restart.
    pub fn at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(self.at.0)
    }
    /// The owning tenant.
    pub fn tenant(&self) -> &TenantId {
        &self.tenant
    }
}
/// Shows the UTC wire text; the tenant is withheld.
impl fmt::Debug for ModifiedSince {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ModifiedSince").field(&self.at).finish()
    }
}

/// A pre-existing contact chosen within one tenant (no Contacts endpoint is implemented).
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct ContactRef {
    pub(crate) tenant: TenantId,
    pub(crate) id: String,
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
#[derive(Clone, Serialize, concord::Debug)]
#[serde(rename_all = "PascalCase")]
#[sensitive]
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

/// A single sales invoice whose Status is always DRAFT and Type always ACCREC.
#[derive(Clone, Serialize, concord::Debug)]
#[serde(rename_all = "PascalCase")]
#[sensitive]
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

/// Returned invoice state. Unmodeled fields, tax calculations and state transitions stay in Xero.
#[derive(Clone, concord::Debug)]
#[sensitive]
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
