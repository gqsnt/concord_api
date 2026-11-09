//! Intercom 2.17 contacts: list, search by exact email, retrieve and create by email.
//!
//! Calls use the generated `send()` and execute once. See the package README for
//! bounded pagination, explicit metadata access and ambiguous-create semantics.

#![doc = include_str!("../README.md")]

pub use concord;
pub use concord_auth::CredentialError;
pub use concord_pagination as pagination;
pub use concord_retry::http::RetryAfterError;

use concord::reqwest::{self, header::AUTHORIZATION};
use concord::{Endpoint, Json, SendWith};

mod error;
mod models;
mod pages;

pub use error::{
    Error, ErrorCode, ErrorDocument, ErrorItem, HeaderNumber, ProviderError, RateLimits,
};
pub use models::{
    Contact, ContactList, CursorNext, CursorPages, EmailSearch, NewContact, PageSize, Role,
};
pub use pages::{ContactPages, PageMeta};

/// Maximum decoded success body size (2 MiB), including a whole contact page.
pub const SUCCESS_BODY_LIMIT: usize = 2 * 1024 * 1024;
/// Maximum provider error document size (64 KiB).
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// One successful response, with explicit access to its value and unredacted head.
#[derive(concord::Debug)]
#[sensitive]
pub struct Response<T> {
    /// The decoded provider value.
    pub value: T,
    /// Status, operation and headers; header access is unredacted.
    #[public]
    pub meta: concord::ResponseMeta,
}

/// Single-execution policy: bounded success decoding and typed non-success documents.
/// No auth refresh, rate wait, retry or canonical-link following occurs here.
#[derive(Default)]
pub struct ResponsePolicy;

impl<T: Send> SendWith<T> for ResponsePolicy {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
                .await;
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        let (value, meta) = received.decode_with_meta().await?;
        Ok(Response { value, meta })
    }
}

concord::api! {
    /// Intercom contacts, pinned to API version 2.17.
    pub client Intercom(
        /// A complete sensitive Authorization header; prefer `with_token`.
        #[sensitive] header(AUTHORIZATION) authorization: reqwest::header::HeaderValue,
    ) {
        base "https://api.intercom.io";
        headers { "intercom-version" = "2.17"; }
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send ResponsePolicy -> Result<Response<_>, Error>;

        /// Contacts in the authenticated workspace.
        scope contacts {
            path "contacts";

            /// A single cursor page. Default size 10; accepted sizes 1 through 150.
            GET list(
                query per_page: PageSize = PageSize::default(),
                #[sensitive] query starting_after?: String,
            ) -> ContactList;

            /// Search by exact email with JSON-body pagination. Default size 50.
            #[read] POST search(body search: EmailSearch) -> ContactList { path "search"; }

            /// Retrieve one contact using its Intercom ID (one encoded path segment).
            GET get(#[sensitive] id: String) -> Contact { path "{id}"; }

            /// Create by email, exactly once. A transport failure may be ambiguous.
            POST create(body contact: NewContact) -> Contact;
        }
    }
}

impl Intercom {
    /// Sends `authorization: Bearer <token>`, built by `concord_auth::bearer`.
    ///
    /// Construct with a raw bearer token, rejecting empty or unsafe header tokens.
    pub fn with_token(http: reqwest::Client, token: &str) -> Result<Self, CredentialError> {
        Ok(Self::new(
            http,
            IntercomConfig {
                authorization: concord_auth::bearer(token)?,
            },
        ))
    }
}

/// A transport with a 15-second timeout and automatic redirects/retries disabled.
/// Callers supplying a different transport own its retry and redirect behavior.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .build()
}

#[cfg(test)]
mod read_operation_tests {
    use super::*;
    #[test]
    fn declared_post_reads_keep_their_method_and_effect() {
        let operation = IntercomContactsSearch::operation();
        assert_eq!(operation.method(), Some("POST"));
        assert!(operation.is_read(), "{}", operation.name());
    }
}
