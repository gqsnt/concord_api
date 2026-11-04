//! A Catalog operation the SDK (`concord-example-sdk`) does not declare, added by another crate:
//! a tenant's CSV export, `GET tenants/{tenant}/exports/{export}`.
//!
//! The SDK's declaration has no such endpoint, and its JSON format does not fit a CSV body:
//!
//! ```compile_fail,E0599
//! fn export(tenant: &concord_example_sdk::CatalogTenant) {
//!     let _ = tenant.item("item-001"); // a declared endpoint: compiles
//!     let _ = tenant.export("items-2026"); // no such generated method
//! }
//! ```
//!
//! This crate uses only public items:
//!
//! - the SDK's [`TenantContext`] builds the native request — base and path prefix,
//!   `tenants/{tenant}`, the export id as one encoded segment, the service headers and the
//!   sensitive key — and supplies the `reqwest::Client`;
//! - this crate adds `Accept: text/csv` and chooses its own representation, the [`Csv`] format,
//!   through Concord's public [`Decoder::new`](concord::Decoder::new);
//! - `decoder.finish(client.execute(request).await)` applies Concord's status, head and body rules
//!   and converts a transport failure (its URL removed).
//!
//! No declared endpoint is prepared as a seed, nothing hidden is read, and errors name this
//! operation ([`EXPORT_OPERATION`]), never a declared endpoint: the decoder is unlabeled, so
//! `concord::Error::endpoint()` is `None`.
//!
//! ```no_run
//! use concord::reqwest;
//! use concord_example_extension::export;
//! use concord_example_sdk::CatalogConfig;
//! use concord_example_sdk::context::CatalogContext;
//!
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! let catalog = CatalogContext::new(reqwest::Client::new(), CatalogConfig { key: "k-1".into() })
//!     .with_base_url("https://edge.example.test/edge/catalog/v1")?;
//! let acme = catalog.tenant("acme");
//! let rows = export(&acme, "items-2026").await?;
//! // The same context serves the declared endpoints:
//! let item = acme.catalog_tenant().item(&rows[0].id).send().await?;
//! # let _ = item; Ok(()) }
//! ```
//!
//! The requests built here are native: their `Debug` and URL show the tenant and export id. Do
//! not log them when those may be secret (see the SDK's `context` module).

mod csv;
mod error;

pub use csv::{Csv, CsvRow, TEXT_CSV};
pub use error::ExtensionError;

use concord::Decoder;
use concord::reqwest::header::{ACCEPT, HeaderValue};
use concord::reqwest::{self, Method};
use concord_example_sdk::context::TenantContext;

/// The name of the export operation in [`ExtensionError`]s.
pub const EXPORT_OPERATION: &str = "catalog-extension: tenant export";

/// The tenant's export `export` as CSV rows: one `GET tenants/{tenant}/exports/{export}`.
///
/// # Errors
///
/// An [`ExtensionError`] naming [`EXPORT_OPERATION`]:
/// [`invalid_request`](ExtensionError::invalid_request) if the tenant or export id is empty,
/// `.` or `..` (nothing is sent); otherwise [`concord`](ExtensionError::concord) — a `Status`
/// error for an unknown export (`404`), `Metadata` for a response that is not `text/csv` (kept
/// unread), `Decode` for a malformed body, `Transport` if reqwest fails.
pub async fn export(tenant: &TenantContext, export: &str) -> Result<Vec<CsvRow>, ExtensionError> {
    export_with(tenant, export, |_| {}).await
}

/// [`export`], with a native edit of the request before its one execution: an extra header, a
/// timeout. The edit cannot change the representation: the [`Csv`] decoder is kept whatever
/// `Accept` the edit sets.
///
/// # Errors
///
/// As [`export`].
pub async fn export_with<F>(
    tenant: &TenantContext,
    export: &str,
    edit: F,
) -> Result<Vec<CsvRow>, ExtensionError>
where
    F: FnOnce(&mut reqwest::Request),
{
    let (http, mut request, decoder) = export_parts(tenant, export)?;
    edit(&mut request);
    decoder
        .finish(http.execute(request).await)
        .await
        .map_err(|e| ExtensionError::call(EXPORT_OPERATION, e))
}

/// The export as native parts, for a caller that executes it itself — to read a response
/// header (`x-export-id`) before decoding, say: the client, the request (with
/// `Accept: text/csv`) and the [`Csv`] decoder.
///
/// Execute the request **once** with the client. Hand a transport failure to the decoder too
/// (`decoder.finish(Err(e))`) rather than returning the native error, whose `Display` and source
/// chain may contain the URL; a response goes to `decoder.decode(response)`. Those return
/// `concord::Error`s with no endpoint name: name the operation in your own error.
///
/// # Errors
///
/// An [`ExtensionError`] with [`invalid_request`](ExtensionError::invalid_request) if the tenant
/// or export id is empty, `.` or `..`.
pub fn export_parts(
    tenant: &TenantContext,
    export: &str,
) -> Result<(reqwest::Client, reqwest::Request, Decoder<Vec<CsvRow>>), ExtensionError> {
    let mut request = tenant
        .request(Method::GET, &["exports", export])
        .map_err(|e| ExtensionError::invalid(EXPORT_OPERATION, e))?;
    request
        .headers_mut()
        .insert(ACCEPT, HeaderValue::from_static(TEXT_CSV));
    let decoder = Decoder::<Vec<CsvRow>>::new::<Csv>();
    Ok((tenant.http().clone(), request, decoder))
}
