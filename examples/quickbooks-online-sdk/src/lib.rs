//! A bounded QuickBooks Online Accounting API slice: realm-bound OAuth 2.0 connections
//! (authorization code and refresh token), customer query and read, invoice create (sent once,
//! with a request ID) and read, and recording a payment against one invoice. Tests run against
//! local fixtures only.
//!
//! The declared client is [`QuickBooks`], one per connected company (realm):
//! `qbo.customers().query(..)`, `qbo.customer(id).read()`, `qbo.invoices().create(..)`,
//! `qbo.invoice(id).read()`, `qbo.payments().create(..)`, each sent with the generated `send()`
//! through the client's [`Connection`], which supplies the realm of every path, its access token
//! and the official HTTPS environment host. [`QuickBooks::set_connection`] replaces all three
//! together at send time; loopback fixture origins stay unchanged.
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
    /// `prepare()` builds the declared request **without** `Authorization`, keeping the generated
    /// base snapshot. `send()` (the connection's policy) selects the connection's environment host
    /// for either official HTTPS host and adds the access token. Fixture origins are kept.
    pub client QuickBooks(
        /// The realm-bound OAuth connection (its `Display` is the realm ID).
        /// `set_connection` replaces realm, credential source and send policy together. Sending
        /// to either official HTTPS host selects this connection's environment, even after a
        /// sandbox/production switch; fixture origins and existing handles keep their snapshots.
        connection: Connection,
    ) {
        base "https://quickbooks.api.intuit.com";
        // `connect` chooses the connection's environment; fixtures validate their origin.
        controls { pub(crate) new; pub(crate) with_base_url; }
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
        #[sensitive] header(concord::reqwest::header::AUTHORIZATION) client_credentials: concord::reqwest::header::HeaderValue,
    ) {
        base "https://oauth.platform.intuit.com";
        path "oauth2/v1/tokens";
        request concord::Form;
        response Json;
        limit TOKEN_BODY_LIMIT;

        /// `POST /oauth2/v1/tokens/bearer`.
        POST bearer(body grant: auth::Grant) -> auth::TokenResponse { path "bearer"; }
    }
}

impl QuickBooks {
    /// Construct for a bare HTTP origin on a literal loopback IP, using synthetic credentials.
    /// The supplied transport must disable redirects and retries, as for [`Self::connect`].
    pub fn for_fixture(
        http: reqwest::Client,
        connection: Connection,
        origin: &str,
    ) -> Result<Self, Error> {
        let client = Self::connect(http, connection).with_base_url(origin)?;
        let url = concord::reqwest::Url::parse(origin).expect("the base URL was validated");
        let loopback = url
            .host_str()
            .and_then(|host| {
                host.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .ok()
            })
            .is_some_and(|ip| ip.is_loopback());
        if url.scheme() != "http" || !loopback || url.path() != "/" {
            return Err(Error::InvalidInput {
                what: "fixture origin",
            });
        }
        Ok(client)
    }

    /// A client for `connection`'s company, at its environment's base URL
    /// (`https://sandbox-quickbooks.api.intuit.com` or `https://quickbooks.api.intuit.com`).
    /// After [`Self::set_connection`], `send()` selects the replacement's environment;
    /// `prepare()` retains the generated base snapshot and does not apply authentication.
    pub fn connect(http: reqwest::Client, connection: Connection) -> Self {
        let base = connection.environment().base_url();
        Self::new(http, QuickBooksConfig { connection })
            .with_base_url(base)
            .expect("the documented base URLs are valid")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_names_compose() {
        QuickBooks::validate_header_names().unwrap();
        TokenEndpoint::validate_header_names().unwrap();
    }
}
