//! A bounded Square Payments slice: create, retrieve and cursor-page payments,
//! plus delayed capture (authorize, then complete or cancel).
//!
//! Every generated `send()` makes one application attempt and returns [`Error`] on failure.
//! Retain a clone of a create operation before sending: after a timeout, explicitly
//! sending that unchanged clone reuses the caller's key and body. Never replace
//! the key to resolve an unknown outcome. A complete or cancel whose outcome is
//! unknown is resolved by re-reading the payment, never by an SDK resend.
//! See the package README for examples.

#![doc = include_str!("../README.md")]

use std::{fmt, num::NonZeroUsize, time::Duration};

pub use concord;
use concord::{Endpoint, Json, SendOperation, SendWith, reqwest};
pub use concord_auth::{CredentialError, bearer};
pub use concord_pagination::{Pages, PagesError, Resume};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde::{Deserialize, Serialize};

mod error;
mod models;
pub use error::{Error, ErrorDocument, ProviderError, SquareError};
pub use models::{
    CompletePayment, CreatePayment, InputError, Money, Payment, PaymentResponse, PaymentsPage,
};

/// The version pinned by the declaration (not a dynamically selected version).
pub const API_VERSION: &str = "2026-09-16";
/// Maximum decoded success document: 1 MiB, including a full list page.
pub const SUCCESS_BODY_LIMIT: usize = 1024 * 1024;
/// Maximum provider error document: 64 KiB.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;

/// Builds a transport with a caller-selected timeout and no redirects or native retries.
///
/// A caller-supplied transport passed to [`Square::new`] must select those policies
/// itself if exact request counts are required.
pub fn transport(timeout: Duration) -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
}

concord::api! {
    /// Square sandbox API, with a caller-owned Bearer header.
    pub client Square(#[sensitive] header(AUTHORIZATION) authorization: HeaderValue) {
        base "https://connect.squareupsandbox.com";
        path "v2";
        headers { "square-version" = "2026-09-16"; }
        format Json;
        limit SUCCESS_BODY_LIMIT;
        send SquarePolicy -> Result<_, Error>;
        /// Every Square operation; [`Error::create_outcome_unknown`] decides per operation
        /// whether a replay rule written for create applies.
        operations SquareOperation;

        /// Payments belonging to the account identified by the Bearer token.
        scope payments {
            path "payments";

            /// Creates a payment using the caller's key in the JSON body: immediate
            /// capture ([`CreatePayment::new`]) or authorization only
            /// ([`CreatePayment::authorization`], status `APPROVED`).
            /// A timeout leaves the outcome unknown; replay only an unchanged operation.
            POST create(body payment: CreatePayment) -> PaymentResponse;

            /// Retrieves one payment; an ID is encoded as exactly one path segment.
            GET get(#[sensitive] payment_id: String) -> PaymentResponse {
                path "{payment_id}";
            }

            /// Reads one page. The cursor is opaque and belongs to this query/account.
            /// Square defaults to 100 results and ignores limits greater than 100.
            GET list(#[sensitive] query cursor?: String, query limit?: u32) -> PaymentsPage;

            /// Completes (captures) an `APPROVED` payment. With a version token, a payment
            /// that changed since is refused: [`ProviderError::is_version_mismatch`].
            /// Square takes no idempotency key here: an unknown outcome
            /// ([`Error::outcome_unknown`]) is resolved by re-reading the payment.
            POST complete(
                #[sensitive] payment_id: String,
                body completion: CompletePayment,
            ) -> PaymentResponse {
                path "{payment_id}/complete";
            }

            /// Cancels (voids) an `APPROVED` payment; no request body. Square takes no
            /// idempotency key here: an unknown outcome ([`Error::outcome_unknown`]) is
            /// resolved by re-reading the payment.
            POST cancel(#[sensitive] payment_id: String) -> PaymentResponse {
                path "{payment_id}/cancel";
            }
        }
    }
}

/// The client's single-attempt policy: strict, bounded JSON and typed HTTP errors.
#[derive(Default)]
pub struct SquarePolicy;

impl<T: Send> SendWith<T> for SquarePolicy {
    type Output = Result<T, Error>;

    async fn send<E: Endpoint<T> + Send>(&self, endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        if !received.status().is_success() {
            let (meta, document) = received
                .document::<Json, ErrorDocument>(ERROR_BODY_LIMIT)
                .await;
            return Err(Error::Provider(Box::new(ProviderError { meta, document })));
        }
        Ok(received.decode().await?)
    }
}

/// The client declares `operations SquareOperation;`, so each generated `send()` hands its
/// variant to the policy; this policy treats every operation alike and sends as
/// [`SendWith`].
impl<T: Send> SendOperation<T, SquareOperation> for SquarePolicy {
    type Output = <Self as SendWith<T>>::Output;

    fn send<E: Endpoint<T> + Send>(
        &self,
        _operation: SquareOperation,
        endpoint: E,
    ) -> impl Future<Output = Self::Output> + Send {
        SendWith::send(self, endpoint)
    }
}

impl SquarePaymentsList {
    /// Walks the listing with a mandatory page bound and explicit start/resume position.
    ///
    /// The position replaces any cursor on this template. Keep the same account,
    /// base URL and limit when resuming. Empty pages with a cursor continue;
    /// an absent or empty cursor completes. A cap or failed page is an error,
    /// preserving the continuation through [`PagesError`]. Nothing is retried. Use `concord_sdk_square_payments::Resume` for the position. `Resume::Start` repeats the payments listing without `cursor`, replacing the template cursor; `Resume::At(key)` takes the `String` response cursor in a returned page's `next`.
    pub fn pages(
        self,
        from: Resume<String>,
        max_pages: NonZeroUsize,
    ) -> Pages<Payment, String, (), Error> {
        Pages::new(from, move |cursor| {
            let request = self.clone().maybe_cursor(cursor);
            async move {
                let page = request.send().await?;
                Ok(concord_pagination::Page::new(
                    page.payments,
                    page.cursor.filter(|value| !value.is_empty()),
                ))
            }
        })
        .max_pages(max_pages)
    }
}
