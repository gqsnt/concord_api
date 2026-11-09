//! A bounded Xero Accounting SDK: page/get invoices and create/get one draft, in one tenant.
//!
//! Uses ordinary Concord declarations and generated `send()` for every HTTP exchange.
//! Live provider calls in this package's evidence: **NOT_RUN**. See README and PROVENANCE.
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

mod auth;
mod error;
mod models;
mod policy;
mod store;

pub use concord;
pub use concord_pagination;
pub use error::{Class, Error, ErrorKind, Problem, ProviderFailure};
pub use models::{
    Amount, ContactRef, DraftInvoice, Invoice, InvoiceRef, LineItem, ModifiedSince, Pagination,
    TenantId,
};
pub use policy::{BODY_LIMIT, ERROR_BODY_LIMIT};
pub use store::{FileStore, Grant};

use concord::reqwest;
use concord_pagination::{Page, Pages, Resume};
use models::{CreateBody, InvoicesDocument, Utc};
use std::{
    num::NonZeroUsize,
    time::{Duration, SystemTime},
};

mod wire {
    use super::*;
    use crate::policy::Policy;
    concord::api! {
        pub client Accounting(origin: concord::BaseUrl, policy:Policy) {
            base origin;
            path "api.xro/2.0";
            format concord::Json;
            limit BODY_LIMIT;
            send policy -> Result<_,Error>;
            scope tenant(#[sensitive] header("xero-tenant-id") tenant:String) {
                GET list(query page:u32,query("pageSize") page_size:u32,modified_since?:Utc) -> InvoicesDocument {
                    path "Invoices";
                    // Absent value: no header. Utc's Display is Xero's documented UTC spelling.
                    headers {"if-modified-since"="{modified_since}";}
                }
                GET get(#[sensitive] id:String) -> InvoicesDocument {path "Invoices/{id}";}
                POST create(body invoice:CreateBody) -> InvoicesDocument {
                    path "Invoices";
                    query {"summarizeErrors"="true";}
                }
            }
        }
    }
}

/// The send policy sets `authorization: Bearer <access_token>` from the current token snapshot
/// on each Accounting attempt. Token refresh sends `authorization: Basic <base64(client_id:client_secret)>`.
/// Tenant operations send `xero-tenant-id` from the selected store-bound tenant ID.
///
/// Tenant-bound Accounting client. Clones share auth flights, concurrency and cooldown state.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct Xero {
    api: wire::Accounting,
    tenant: TenantId,
}
impl Xero {
    /// Use Xero's Accounting and OAuth origins. No requests occur during construction.
    pub fn new(store: FileStore, client_secret: &str) -> Result<Self, Error> {
        Self::with_origins(
            store,
            client_secret,
            "https://api.xero.com",
            "https://identity.xero.com",
        )
    }
    /// Configure service origins, normally local fixtures. HTTPS or loopback HTTP only;
    /// paths, queries, fragments and URL credentials are refused. Existing handles retain origins.
    pub fn with_origins(
        store: FileStore,
        client_secret: &str,
        accounting: &str,
        oauth: &str,
    ) -> Result<Self, Error> {
        validate_origin(accounting)?;
        validate_origin(oauth)?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| Error::input("HTTP transport configuration failed"))?;
        let tenant = store.tenant().clone();
        let auth = auth::Auth::new(store, client_secret, oauth, http.clone())?;
        let api = wire::Accounting::new(
            http,
            wire::AccountingConfig {
                origin: concord::BaseUrl::parse(accounting)?,
                policy: policy::Policy::new(auth),
            },
        );
        Ok(Self { api, tenant })
    }
    /// Enter only the tenant authorized for this credential journal.
    pub fn tenant(&self, tenant: &TenantId) -> Result<Tenant, Error> {
        if tenant != &self.tenant {
            return Err(Error::new(ErrorKind::TenantMismatch));
        }
        Ok(Tenant {
            api: self.api.tenant(tenant.as_str()),
            id: tenant.clone(),
        })
    }
}
fn validate_origin(value: &str) -> Result<(), Error> {
    let url = reqwest::Url::parse(value).map_err(|_| Error::input("invalid service origin"))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if url.host_str().is_none()
        || !(url.scheme() == "https" || (local && url.scheme() == "http"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || url.port() == Some(0)
    {
        return Err(Error::input(
            "service origin must be HTTPS or loopback HTTP without credentials or path",
        ));
    }
    Ok(())
}

/// Fixed tenant context, carrying no publicly mutable credential or tenant setters.
#[derive(Clone, concord::Debug)]
#[sensitive]
pub struct Tenant {
    api: wire::AccountingTenant,
    id: TenantId,
}
impl Tenant {
    /// The immutable context chosen by the caller.
    pub fn id(&self) -> &TenantId {
        &self.id
    }
    /// Bind a known invoice ID to this tenant. The application asserts where the ID came from.
    pub fn invoice(&self, id: impl Into<String>) -> Result<InvoiceRef, Error> {
        let id = id.into();
        if !models::uuid(&id) {
            return Err(Error::input("invoice ID must be a UUID"));
        }
        Ok(InvoiceRef {
            tenant: self.id.clone(),
            id: id.to_ascii_lowercase(),
        })
    }
    /// Bind an authorized contact ID to this tenant; no contact lookup is performed.
    pub fn contact(&self, id: impl Into<String>) -> Result<ContactRef, Error> {
        let id = id.into();
        if !models::uuid(&id) {
            return Err(Error::input("contact ID must be a UUID"));
        }
        Ok(ContactRef {
            tenant: self.id.clone(),
            id: id.to_ascii_lowercase(),
        })
    }
    /// Build a sales draft from this tenant's existing contact and 1–100 lines.
    pub fn draft(&self, contact: ContactRef, lines: Vec<LineItem>) -> Result<DraftInvoice, Error> {
        self.check(&contact.tenant)?;
        DraftInvoice::new(contact, lines)
    }
    fn check(&self, id: &TenantId) -> Result<(), Error> {
        if id != &self.id {
            return Err(Error::new(ErrorKind::TenantMismatch));
        }
        Ok(())
    }
    /// Build a page operation with page >= 1 and page size in 1..=1000, rejecting clamping.
    pub fn list(&self, page: u32, page_size: u32) -> Result<ListInvoices, Error> {
        self.list_filtered(page, page_size, None)
    }
    /// Bind an incremental-sync point to this tenant. The instant is truncated to the whole
    /// UTC second and must lie in 1970..=9999. To sync incrementally, take `SystemTime::now()`
    /// **before** a sync's first request and, once that sync completes, bind it here for the next.
    pub fn modified_since(&self, at: SystemTime) -> Result<ModifiedSince, Error> {
        ModifiedSince::new(self.id.clone(), at)
    }
    /// [`Tenant::list`] restricted by Xero's `If-Modified-Since` header (`yyyy-mm-ddThh:mm:ss`,
    /// UTC). A sync point from another tenant fails before I/O.
    pub fn list_modified_since(
        &self,
        since: &ModifiedSince,
        page: u32,
        page_size: u32,
    ) -> Result<ListInvoices, Error> {
        self.check(&since.tenant)?;
        self.list_filtered(page, page_size, Some(since.at))
    }
    fn list_filtered(
        &self,
        page: u32,
        page_size: u32,
        since: Option<Utc>,
    ) -> Result<ListInvoices, Error> {
        if page == 0 || !(1..=1000).contains(&page_size) {
            return Err(Error::input("invalid invoice paging parameters"));
        }
        Ok(ListInvoices {
            tenant: self.clone(),
            page,
            page_size,
            since,
        })
    }
    /// Retrieve a chosen invoice; a reference from another tenant fails before I/O.
    pub fn get(&self, invoice: &InvoiceRef) -> Result<GetInvoice, Error> {
        self.check(&invoice.tenant)?;
        Ok(GetInvoice {
            tenant: self.clone(),
            invoice: invoice.clone(),
        })
    }
    /// Create exactly one draft, without automatic write replay.
    pub fn create(&self, draft: DraftInvoice) -> Result<CreateInvoice, Error> {
        self.check(&draft.tenant)?;
        Ok(CreateInvoice {
            tenant: self.clone(),
            draft,
        })
    }
    /// Walk bounded pages. Metadata is authoritative when present; older responses continue
    /// until an empty page, even after a short nonempty page. A cap returns a typed partial result.
    pub fn pages(
        &self,
        from: Option<PageCursor>,
        page_size: u32,
        max_pages: NonZeroUsize,
    ) -> Result<InvoicePages, Error> {
        self.traverse(from, page_size, max_pages, None)
    }
    /// [`Tenant::pages`] restricted by `since`, which every page of the traversal sends
    /// unchanged. A continuation resumes only with the same sync point, page size and tenant; a
    /// failed first page resumes from `None` with the same `since`. Completion (`Ok(None)` from
    /// `next_page`, or `collect_complete`) is the only point at which to advance the sync point.
    pub fn pages_modified_since(
        &self,
        since: &ModifiedSince,
        from: Option<PageCursor>,
        page_size: u32,
        max_pages: NonZeroUsize,
    ) -> Result<InvoicePages, Error> {
        self.check(&since.tenant)?;
        self.traverse(from, page_size, max_pages, Some(since.at))
    }
    fn traverse(
        &self,
        from: Option<PageCursor>,
        page_size: u32,
        max_pages: NonZeroUsize,
        since: Option<Utc>,
    ) -> Result<InvoicePages, Error> {
        self.list_filtered(1, page_size, since)?;
        if let Some(key) = &from {
            self.check_cursor(key, page_size, since)?;
        }
        let tenant = self.clone();
        Ok(Pages::new(
            from.map_or(Resume::Start, Resume::At),
            move |key: Option<PageCursor>| {
                let tenant = tenant.clone();
                async move {
                    if let Some(key) = &key {
                        tenant.check_cursor(key, page_size, since)?;
                    }
                    let page = key.map_or(1, |key| key.page);
                    let fetched = tenant.list_filtered(page, page_size, since)?.send().await?;
                    Ok(Page {
                        items: fetched.invoices,
                        next: fetched.next,
                        meta: fetched.pagination,
                    })
                }
            },
        )
        .max_pages(max_pages))
    }
    fn check_cursor(
        &self,
        key: &PageCursor,
        page_size: u32,
        since: Option<Utc>,
    ) -> Result<(), Error> {
        self.check(&key.tenant)?;
        if key.page_size != page_size {
            return Err(Error::input("continuation page size mismatch"));
        }
        if key.since != since {
            return Err(Error::input("continuation modified-since mismatch"));
        }
        Ok(())
    }
}

/// Owned traversal with safe diagnostics and typed incomplete/error reports.
pub type InvoicePages = Pages<Invoice, PageCursor, Option<Pagination>, Error>;
/// Tenant, page-size and modified-since bound continuation; its fields cannot be reassigned by
/// callers.
#[derive(Clone, PartialEq, Eq, concord::Debug)]
#[sensitive]
pub struct PageCursor {
    tenant: TenantId,
    page: u32,
    page_size: u32,
    since: Option<Utc>,
}
impl PageCursor {
    /// The next one-based page number.
    pub fn page(&self) -> u32 {
        self.page
    }
}
/// A page with its validated continuation and optional modern metadata.
#[derive(Debug)]
pub struct InvoicePage {
    /// Tenant-bound invoices.
    pub invoices: Vec<Invoice>,
    /// Next request, or None only on provider exhaustion.
    pub next: Option<PageCursor>,
    /// The provider's optional paging metadata.
    pub pagination: Option<Pagination>,
}
/// A validated list endpoint operation.
#[derive(Clone, Debug)]
pub struct ListInvoices {
    tenant: Tenant,
    page: u32,
    page_size: u32,
    since: Option<Utc>,
}
impl ListInvoices {
    /// Execute the generated Concord list endpoint, then validate provider continuation semantics.
    pub async fn send(self) -> Result<InvoicePage, Error> {
        let doc = self
            .tenant
            .api
            .list(self.page, self.page_size)
            .maybe_modified_since(self.since)
            .send()
            .await?;
        check_envelope(&doc)?;
        let n = doc.invoices.len() as u64;
        if n > u64::from(self.page_size) {
            return Err(contract("page exceeds requested page size"));
        }
        let more = if let Some(meta) = doc.pagination {
            let expected = meta.item_count.div_ceil(u64::from(self.page_size));
            let offset = u64::from(self.page - 1) * u64::from(self.page_size);
            let expected_items = meta
                .item_count
                .saturating_sub(offset)
                .min(u64::from(self.page_size));
            if meta.page != self.page
                || meta.page_size != self.page_size
                || u64::from(meta.page_count) != expected
                || n != expected_items
                || (meta.page_count == 0 && self.page != 1)
                || (meta.page_count > 0 && self.page > meta.page_count)
            {
                return Err(contract("inconsistent invoice pagination metadata"));
            }
            self.page < meta.page_count
        } else {
            n > 0
        };
        let next = if more {
            Some(PageCursor {
                tenant: self.tenant.id.clone(),
                page: self
                    .page
                    .checked_add(1)
                    .ok_or_else(|| contract("invoice page overflow"))?,
                page_size: self.page_size,
                since: self.since,
            })
        } else {
            None
        };
        let invoices = doc
            .invoices
            .into_iter()
            .map(|v| v.bind(&self.tenant.id))
            .collect::<Result<_, _>>()?;
        Ok(InvoicePage {
            invoices,
            next,
            pagination: doc.pagination,
        })
    }
}
/// A validated individual-invoice endpoint operation.
#[derive(Clone, Debug)]
pub struct GetInvoice {
    tenant: Tenant,
    invoice: InvoiceRef,
}
impl GetInvoice {
    /// Retrieve exactly the requested invoice in this tenant. A mismatched ID is refused.
    pub async fn send(self) -> Result<Invoice, Error> {
        let doc = self.tenant.api.get(self.invoice.id.clone()).send().await?;
        let invoice = single(doc, &self.tenant.id)?;
        if invoice.reference != self.invoice {
            return Err(contract("retrieved invoice ID differs from request"));
        }
        Ok(invoice)
    }
}
/// A validated create-draft operation. Clone does not grant replay permission.
#[derive(Clone, Debug)]
pub struct CreateInvoice {
    tenant: Tenant,
    draft: DraftInvoice,
}
impl CreateInvoice {
    /// Send once and require exactly one DRAFT ACCREC result. Unknown delivery, 5xx and
    /// unreadable or inconsistent success bodies mark the write uncertain.
    pub async fn send(self) -> Result<Invoice, Error> {
        let doc = self
            .tenant
            .api
            .create(CreateBody {
                invoices: vec![self.draft],
            })
            .send()
            .await
            .map_err(|e| {
                let uncertain = match e.kind() {
                    ErrorKind::Concord(e) => e.kind() != concord::ErrorKind::Build,
                    ErrorKind::Provider(p) => p.meta.status().is_server_error(),
                    ErrorKind::Deadline => true,
                    _ => false,
                };
                if uncertain { e.uncertain() } else { e }
            })?;
        let invoice = single(doc, &self.tenant.id).map_err(|e| {
            if matches!(e.kind(), ErrorKind::InvoiceValidation) {
                e
            } else {
                e.uncertain()
            }
        })?;
        if invoice.status != "DRAFT" || invoice.invoice_type != "ACCREC" {
            return Err(contract("created invoice is not a draft sales invoice").uncertain());
        }
        Ok(invoice)
    }
}
fn contract(s: &'static str) -> Error {
    Error::new(ErrorKind::Contract(s))
}
fn check_envelope(doc: &InvoicesDocument) -> Result<(), Error> {
    if doc.status.as_deref().is_some_and(|s| s != "OK") {
        return Err(contract("invoice envelope status is not OK"));
    }
    Ok(())
}
fn single(mut doc: InvoicesDocument, tenant: &TenantId) -> Result<Invoice, Error> {
    check_envelope(&doc)?;
    if doc.invoices.len() != 1 {
        return Err(contract("expected exactly one invoice"));
    }
    doc.invoices.remove(0).bind(tenant)
}
