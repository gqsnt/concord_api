//! A bounded QuickBooks Online Accounting API slice: realm-bound OAuth 2.0 connections
//! (authorization code and refresh token), customer query and read, invoice create (sent once,
//! with a request ID) and read, and recording a payment against one invoice. Tests run against
//! local fixtures only.
//!
//! The declared client is [`QuickBooks`], one per connected company (realm):
//! `qbo.customers().query(..)`, `qbo.customer(id).read()`, `qbo.invoices().create(..)`,
//! `qbo.invoice(id).read()`, `qbo.payments().create(..)`, each sent with the generated `send()`
//! through the client's [`Connection`], which supplies the realm of every path, its access token
//! and the official HTTPS environment host. [`QuickBooks::set_connection`] replaces the
//! connection at once (`prepare()` uses its realm and base); the environment host and token are
//! selected from that connection; `Connection::for_fixture` selects a loopback fixture.
//! [`customer_pages`] walks a query. See the package README for failure semantics and
//! exclusions.

mod auth;
mod error;
mod format;
mod models;
mod pages;
mod payment;
mod policy;
mod query;

pub use concord;
pub use concord_pagination;

pub use auth::{
    ACCOUNTING_SCOPE, AuthorizationCallback, AuthorizationCode, Connection, Environment, OAuthApp,
    RefreshToken,
};
pub use error::{
    AuthorizationError, AuthorizationKind, CallbackError, Error, Fault, FaultError,
    PaginationError, ProviderError, UncertainCause, UncertainWrite,
};
pub use format::{FaultInSuccess, QboJson};
pub use models::{
    Customer, CustomerPage, CustomerQueryResponse, CustomerResponse, Decimal, EmailAddress,
    EntityId, Invoice, InvoiceResponse, Line, MetaData, NewInvoice, RealmId, Reference, RequestId,
    SalesItemLine, SalesItemLineDetail,
};
pub use pages::customer_pages;
pub use payment::{LinkedTxn, NewPayment, Payment, PaymentLine, PaymentResponse};
pub use query::{Activity, CustomerQuery};

use concord::Json;
use concord::reqwest;

/// The largest successful query body read: 8 MiB (up to 1000 customers per page).
pub const QUERY_BODY_LIMIT: usize = 8 * 1024 * 1024;
/// The largest successful single-entity body read (customer, invoice, payment): 1 MiB.
pub const ENTITY_BODY_LIMIT: usize = 1024 * 1024;
/// The largest error body read (a `Fault`, an OAuth error): 64 KiB.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;
/// The largest token endpoint response read: 64 KiB.
pub const TOKEN_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// The QuickBooks Online Accounting API (v3, minor version 75) for **one** company. The
    /// [`Connection`] is both the realm of every path (`/v3/company/{realmId}/..`) and the
    /// source of its access token, so a request and its credential cannot name different realms.
    /// Build it with [`QuickBooks::connect`].
    ///
    /// `prepare()` builds the declared request from the connection's base and realm.
    /// `send()` (the connection's policy) adds its access token.
    pub client QuickBooks(
        /// The realm-bound OAuth connection (its `Display` is the realm ID).
        /// `set_connection` replaces realm, base, credential source and send policy together.
        /// Earlier handles, scopes and requests keep their connection snapshot.
        mut connection: Connection,
    ) {
        base connection;
        error crate::FaultResponse in concord::Json;
        path "v3/company/{connection}";
        query { "minorversion" = "75"; }
        request Json;
        response QboJson;
        limit ENTITY_BODY_LIMIT;
        send connection -> Result<_, Error>;

        /// The company's customers.
        scope customers {
            /// Query a customer: `GET /query?query=SELECT * FROM Customer ..`. Walk every page
            /// with [`customer_pages`].
            GET query(
                /// The select statement (withheld from Concord's output: it can hold names).
                #[sensitive] query("query") statement: CustomerQuery,
            ) -> CustomerQueryResponse {
                path "query";
                limit QUERY_BODY_LIMIT;
            }
        }

        /// One customer.
        scope customer(id: EntityId) {
            path "customer/{id}";

            /// Read a customer.
            GET read() -> CustomerResponse;
        }

        /// The company's invoices.
        scope invoices {
            path "invoice";

            /// Create an invoice. Sent exactly once; a possibly-applied create is
            /// [`Error::Uncertain`] with its request ID, to replay deliberately.
            POST create(
                body invoice: NewInvoice,
                /// `requestid`: a repeated request with this ID gets the original response.
                /// Fresh for each `create(..)` call; keep it (or set it) to replay.
                query("requestid") request_id: RequestId = RequestId::generate(),
            ) -> InvoiceResponse;
        }

        /// One invoice.
        scope invoice(id: EntityId) {
            path "invoice/{id}";

            /// Read an invoice.
            GET read() -> InvoiceResponse;
        }

        /// The company's payments against invoices.
        scope payments {
            path "payment";

            /// Record a payment against one invoice, sent once. A possibly-applied payment is
            /// [`Error::Uncertain`] with its request ID. Read the invoice again for its balance.
            POST create(
                body payment: NewPayment,
                /// Fresh for each create; retain it to reconcile an uncertain result.
                query("requestid") request_id: RequestId = RequestId::generate(),
            ) -> PaymentResponse;
        }
    }
}

concord::api! {
    /// Intuit's OAuth 2.0 token endpoint: both grants, client credentials in Basic
    /// authentication, a form body (secrets never in the URL).
    pub(crate) client TokenEndpoint(
        origin: concord::BaseUrl,
        #[sensitive] header(concord::reqwest::header::AUTHORIZATION) client_credentials: concord::reqwest::header::HeaderValue,
    ) {
        base origin;
        error crate::auth::OAuthError in concord::Json;
        path "oauth2/v1/tokens";
        request concord::Form;
        response Json;
        limit TOKEN_BODY_LIMIT;

        /// `POST /oauth2/v1/tokens/bearer`.
        POST bearer(body grant: auth::Grant) -> auth::TokenResponse { path "bearer"; }
    }
}

impl QuickBooks {
    /// A client for `connection`'s company, at its environment's base URL
    /// (`https://sandbox-quickbooks.api.intuit.com` or `https://quickbooks.api.intuit.com`).
    /// After [`Self::set_connection`], `prepare()` uses the replacement's base and realm;
    /// `send()` applies its authentication.
    pub fn connect(http: reqwest::Client, connection: Connection) -> Self {
        Self::new(http, QuickBooksConfig { connection })
    }
}

/// A transport for this SDK: no transport-level retries (a write is sent exactly once), no
/// redirects (no credential follows a hop), a 30-second timeout per request. An application's
/// own client must have the same restrictions for the failure semantics to hold.
///
/// # Errors
///
/// reqwest's, if the client cannot be built.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
}

/// Compiles the README examples without making provider requests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeExamples;

/// A replacement connection remains required and replaces the whole value on a new snapshot.
///
/// ```
/// use concord_sdk_quickbooks_online::{Connection, QuickBooks, QuickBooksConfig};
///
/// fn config(connection: Connection) -> QuickBooksConfig {
///     QuickBooksConfig { connection }
/// }
///
/// fn replace(client: QuickBooks, connection: Connection) -> QuickBooks {
///     client.set_connection(connection)
/// }
/// ```
///
/// `mut` does not give a required send-policy value a default.
///
/// ```compile_fail,E0063
/// use concord_sdk_quickbooks_online::QuickBooksConfig;
/// let config = QuickBooksConfig {};
/// ```
///
/// A required scope ID identifies the selected customer; select another scope to change it.
///
/// ```compile_fail,E0599
/// use concord_sdk_quickbooks_online::{EntityId, QuickBooks};
///
/// fn change_customer(client: &QuickBooks, id: EntityId, other: EntityId) {
///     client.customer(id).set_id(other);
/// }
/// ```
///
/// The invoice scope has the same rule.
///
/// ```compile_fail,E0599
/// use concord_sdk_quickbooks_online::{EntityId, QuickBooks};
///
/// fn change_invoice(client: &QuickBooks, id: EntityId, other: EntityId) {
///     client.invoice(id).set_id(other);
/// }
/// ```
#[cfg(doctest)]
pub struct ConstructionExamples;

/// The provider error document used by declared request handling.
pub use error::FaultResponse;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_names_compose() {
        QuickBooks::validate_header_names().unwrap();
        TokenEndpoint::validate_header_names().unwrap();
    }
}

/// The connection supplies the Accounting destination.
///
/// ```compile_fail,E0599
/// use concord_sdk_quickbooks_online::QuickBooks;
/// fn redirect(api: QuickBooks) { let _ = api.with_base_url("https://evil.example"); }
/// ```
#[cfg(doctest)]
pub struct DestinationConstructionExamples;
