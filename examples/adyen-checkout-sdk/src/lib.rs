//! Adyen Checkout v71 sandbox payment methods, payments, payment details and manual capture.
//!
//! Ordinary generated `send()` enforces the SDK's idempotency policy. Native `prepare()`
//! performs no I/O and deliberately bypasses that policy if executed directly.
//! Actions are data: this crate never visits a redirect URL or runs a 3DS challenge.

#![doc = include_str!("../README.md")]

mod error;
mod models;
mod policy;

pub use concord;
use concord::{Json, reqwest};
pub use error::{Error, HttpClass, Problem, ProviderFailure, Reconciliation, ReconciliationReason};
pub use models::*;
pub use policy::{Confirm, Response, SendPolicy};

/// Maximum buffered success document: 256 KiB.
pub const RESPONSE_LIMIT: usize = 256 * 1024;
/// Maximum buffered provider error document: 16 KiB.
pub const ERROR_LIMIT: usize = 16 * 1024;
/// Maximum encoded request retained per ledger entry: 256 KiB.
pub const REQUEST_LIMIT: usize = 256 * 1024;
/// Conservative minimum provider key retention used by this SDK: seven days.
pub const GUARANTEE_WINDOW: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

concord::api! {
    /// A bounded Checkout API client. Share the policy across handles for the same company.
    pub client Checkout(
        #[sensitive] header("x-api-key") api_key: String,
        policy: SendPolicy,
    ) {
        base "https://checkout-test.adyen.com";
        format Json;
        limit RESPONSE_LIMIT;
        send policy -> Result<Response<_>, Error>;

        /// Checkout API version 71; replacing the base does not remove the version.
        scope v71 {
            path "v71";

            /// Lists available methods for a transaction; one complete response, no pagination.
            POST payment_methods(
                #[sensitive] header("idempotency-key") key: String,
                body request: MethodsRequest,
            ) -> MethodsResponse { path "paymentMethods"; }

            /// Starts a sandbox payment. Reuse this operation or key only with identical inputs.
            POST payments(
                #[sensitive] header("idempotency-key") key: String,
                body request: PaymentRequest,
            ) -> PaymentResponse { path "payments"; }

            /// Submits caller-obtained details for an existing action. Use a new key for this step.
            POST details(
                #[sensitive] header("idempotency-key") key: String,
                body request: DetailsRequest,
            ) -> PaymentResponse { path "payments/details"; }

            /// An existing payment, by the `pspReference` Adyen returned for it.
            scope payment(#[sensitive] psp_reference: String) {
                path "payments/{psp_reference}";

                /// Requests manual capture of an authorised payment (full or partial amount).
                /// Adyen answers `received`; the outcome is asynchronous. Use a new key per
                /// capture and reuse it only with identical inputs.
                POST captures(
                    #[sensitive] header("idempotency-key") key: String,
                    body request: CaptureRequest,
                ) -> CaptureReceived { path "captures"; }
            }
        }
    }
}

impl Checkout {
    /// Constructs a test-environment client with redirects and reqwest retries disabled.
    ///
    /// `policy` must be shared for all credentials belonging to one company. Its ledger is
    /// process-local; the application owns durable key/request storage across restarts.
    pub fn sandbox(api_key: impl Into<String>, policy: SendPolicy) -> Result<Self, Error> {
        let api_key = api_key.into();
        if api_key.is_empty() || !api_key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::InvalidInput(
                "API key must be a nonempty ASCII token",
            ));
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|_| Error::InvalidInput("HTTP client construction failed"))?;
        Ok(Self::new(http, CheckoutConfig { api_key, policy }))
    }
}
