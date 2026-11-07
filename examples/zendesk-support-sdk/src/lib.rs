//! A bounded Zendesk Support slice (Ticketing API v2): list tickets with cursor pagination, show
//! one ticket, create a ticket with an idempotency key, and safely update a ticket. Tests run against local fixtures only.
//!
//! The declared client is [`Zendesk`]: `api.tickets().list()`, `api.tickets().create(..)`,
//! `api.ticket(id).show()`, `api.ticket(id).update(..)`, each sent with the generated `send()` through [`SendPolicy`].
//! [`ticket_pages`] walks a listing. See the package README for failure semantics and
//! exclusions.

mod codecs;
mod error;
mod models;
mod pagination;
mod policy;

pub use concord;
pub use concord_pagination;
pub use concord_retry;

pub use codecs::CreatedTicketFormat;
pub use error::{
    Error, ErrorDocument, HttpError, PaginationError, Problem, UncertainCause, UncertainWrite,
};
pub use models::{
    CreatedTicket, CursorLinks, CursorMeta, IdempotencyKey, IdempotencyLookup, NewTicket, PageSize,
    Priority, SortField, Status, Ticket, TicketPage, TicketResponse, TicketSort, TicketType,
    TicketUpdate,
};
pub use pagination::ticket_pages;
pub use policy::SendPolicy;

use concord::Json;
use concord::reqwest::{self, header::HeaderValue};

/// The largest successful List Tickets body read: 8 MiB (100 tickets per page).
pub const PAGE_BODY_LIMIT: usize = 8 * 1024 * 1024;
/// The largest successful single-ticket body read (show, create, update): 1 MiB.
pub const TICKET_BODY_LIMIT: usize = 1024 * 1024;
/// The largest error body read: 64 KiB.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// Zendesk Support, `https://{subdomain}.zendesk.com/api/v2`, with an OAuth access token.
    pub client Zendesk(
        /// The account subdomain (`acme` for `acme.zendesk.com`).
        subdomain: String,
        /// `Bearer {access_token}`; build it with [`Zendesk::connect`] or
        /// `concord_auth::bearer(token)`.
        #[sensitive] header("authorization") authorization: HeaderValue,
        /// How generated `send()` treats reads and writes.
        policy: SendPolicy,
    ) {
        base "https://zendesk.com";
        path "api/v2";
        subdomain "{subdomain}";
        format Json;
        limit PAGE_BODY_LIMIT;
        send policy -> Result<_, Error>;

        /// The ticket collection.
        scope tickets {
            path "tickets";

            /// List Tickets with cursor pagination (`page[size]` is always sent, so the offset
            /// mode is never selected). Walk every page with [`ticket_pages`].
            GET list(
                /// `page[size]`, at most 100.
                query("page[size]") page_size: PageSize = PageSize::MAX,
                /// `page[after]`: the previous page's `meta.after_cursor`.
                query("page[after]") after?: String,
                /// `sort`: `updated_at`, `id` or `status`, `-` for descending.
                query sort?: TicketSort,
                /// `external_id`: only tickets with this external id.
                query external_id?: String,
            ) -> TicketPage;

            /// Create Ticket. Sent once by [`SendPolicy`]; a possibly-applied create is
            /// [`Error::Uncertain`] with this key, to replay deliberately.
            POST create(
                body ticket: NewTicket,
                /// `Idempotency-Key`: the same key and body within two hours return the original
                /// ticket instead of creating another.
                idempotency_key: IdempotencyKey,
            ) -> CreatedTicket {
                headers { "idempotency-key" = "{idempotency_key}"; }
                response CreatedTicketFormat;
                limit TICKET_BODY_LIMIT;
            }
        }

        /// One ticket.
        scope ticket(id: u64) {
            path "tickets/{id}";
            limit TICKET_BODY_LIMIT;

            /// Show Ticket.
            GET show() -> TicketResponse;

            /// Update status and add a comment using the last-read ticket's update stamp.
            /// Pass the same ticket's id to this scope. A collision is [`Error::Collision`].
            PUT update(body ticket: TicketUpdate) -> TicketResponse;
        }
    }
}

impl Zendesk {
    /// A client for `https://{subdomain}.zendesk.com/api/v2` with an OAuth access token and the
    /// default [`SendPolicy`]. Obtaining and refreshing the token is the application's.
    ///
    /// # Errors
    ///
    /// [`Error::Credential`] if the token is not a valid bearer credential.
    pub fn connect(
        http: reqwest::Client,
        subdomain: impl Into<String>,
        oauth_token: &str,
    ) -> Result<Self, Error> {
        let authorization = concord_auth::bearer(oauth_token).map_err(Error::Credential)?;
        Ok(Self::new(
            http,
            ZendeskConfig {
                subdomain: subdomain.into(),
                authorization,
                policy: SendPolicy::default(),
            },
        ))
    }
}

/// A transport for this SDK: no transport-level retries (writes are sent exactly once), no
/// redirects, a 30-second timeout per request. An application's own client must have the same
/// restrictions (no retries, no redirects) for the failure semantics to hold.
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
