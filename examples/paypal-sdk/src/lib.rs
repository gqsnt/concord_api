//! A bounded PayPal REST slice for the sandbox: client-credentials access tokens, Orders v2
//! (create an order, show it, capture it once the buyer approved it) and Payments v2 (show a
//! capture), plus authorize-then-capture or void. Tests use local fixtures only.
//!
//! The declared client is [`PayPal`]:
//!
//! - `paypal.orders().create(order)` — Create order (`POST /v2/checkout/orders`);
//! - `paypal.order(id).get()` — Show order details (`GET /v2/checkout/orders/{id}`);
//! - `paypal.order(id).capture(CaptureRequest::new())` — Capture payment for order
//!   (`POST /v2/checkout/orders/{id}/capture`);
//! - `paypal.capture(id).get()` — Show captured payment details
//!   (`GET /v2/payments/captures/{id}`).
//! - `paypal.order(id).authorize(AuthorizeRequest::new())` — authorize an approved order;
//!   inspect `order.authorizations()` for authorization IDs and statuses.
//! - `paypal.order(id).get_authorized()` — show the order with authorization details;
//! - `paypal.authorization(id).capture(AuthorizationCaptureRequest::new())` — capture funds;
//! - `paypal.authorization(id).void()` — release an authorization (returns `()`).
//!
//! Each generated `send()` goes through the client's [`Session`]: it obtains (or reuses) an
//! access token with the client-credentials grant, applies it, and decides whether an outcome may
//! be repeated. Reads are repeated after a transport failure or a `5xx`; documented writes
//! only when they carry a [`RequestId`]; a write that may have been applied is
//! [`Error::Uncertain`]. See the package README for the full failure semantics and exclusions.

mod auth;
mod error;
mod models;
mod policy;

pub use concord;
pub use concord_retry;

pub use auth::{Credentials, IdentityError, TokenError};
pub use error::{ApiError, Error, ErrorDetail, HttpError, Uncertain, UncertainCause};
pub use models::{
    Authorization, AuthorizationCaptureRequest, AuthorizationId, AuthorizationStatus,
    AuthorizeRequest, AuthorizedOrder, AuthorizedPayments, AuthorizedPurchaseUnit, Capture,
    CaptureId, CaptureRequest, CaptureStatus, Intent, Link, MAX_PURCHASE_UNITS, Money, Order,
    OrderId, OrderRequest, OrderStatus, Payments, PurchaseUnit, PurchaseUnitRequest, RequestId,
    StatusDetails,
};
pub use policy::Session;

use std::time::{Duration, Instant};

use concord::Json;
use concord::reqwest;

/// The sandbox base URL, also the base of the declaration.
pub const SANDBOX: &str = "https://api-m.sandbox.paypal.com";

/// The largest successful order or capture body read: 1 MiB.
pub const BODY_LIMIT: usize = 1024 * 1024;
/// The largest error document read: 64 KiB.
pub const ERROR_BODY_LIMIT: usize = 64 * 1024;
/// The largest token document read: 64 KiB.
pub const TOKEN_BODY_LIMIT: usize = 64 * 1024;

concord::api! {
    /// PayPal REST, sandbox (`https://api-m.sandbox.paypal.com`): Orders v2 and Payments v2
    /// captures and authorizations. Build it with [`PayPal::sandbox`] or [`PayPal::builder`];
    /// every `send()` is authenticated and classified by its [`Session`].
    pub client PayPal(
        /// Credentials, token cache and repetition rules; [`PayPal::builder`] creates it.
        session: Session,
    ) {
        base "https://api-m.sandbox.paypal.com";
        format Json;
        limit BODY_LIMIT;
        send session -> Result<_, Error>;
        /// Every PayPal operation, as the [`Session`] classifies it (read or write, and whether a
        /// write may be repeated with its `PayPal-Request-Id`).
        operations PayPalOperation;

        /// Orders v2 collection.
        scope orders {
            path "v2/checkout/orders";

            /// Create order. With a [`RequestId`] (`set_request_id`) a timed-out or `5xx`
            /// create is sent again with the same ID; without one it is sent once and a
            /// possibly-applied create is [`Error::Uncertain`].
            POST create(
                body order: OrderRequest,
                /// `PayPal-Request-Id`: PayPal performs the create at most once per ID within
                /// its retention (6 hours by default).
                request_id?: RequestId,
            ) -> Order {
                headers {
                    "paypal-request-id" = "{request_id}";
                    "prefer" = "return=representation";
                }
            }
        }

        /// One order.
        scope order(id: OrderId) {
            path "v2/checkout/orders/{id}";

            /// Show order details (its current state).
            GET get() -> Order;

            /// Show the same order details as `get()`, decoding the authorization view so a
            /// caller can reconcile an uncertain authorization without repeating the write.
            GET get_authorized() -> AuthorizedOrder;

            /// Authorize an `AUTHORIZE` order after buyer approval. A lost response is
            /// uncertain; repeated only with a caller-supplied [`RequestId`].
            POST authorize(
                body request: AuthorizeRequest,
                /// The ID of this logical authorization, distinct from the create ID.
                request_id?: RequestId,
            ) -> AuthorizedOrder {
                path "authorize";
                headers {
                    "paypal-request-id" = "{request_id}";
                    "prefer" = "return=representation";
                }
            }

            /// Capture payment for an order the buyer approved. Sent once unless it carries a
            /// [`RequestId`]; a capture that may have happened is [`Error::Uncertain`].
            POST capture(
                body request: CaptureRequest,
                /// `PayPal-Request-Id`: a repeated capture with the same ID within PayPal's
                /// retention answers `200` with the stored result instead of capturing again.
                request_id?: RequestId,
            ) -> Order {
                path "capture";
                headers {
                    "paypal-request-id" = "{request_id}";
                    "prefer" = "return=representation";
                }
            }
        }

        /// One authorized payment (Payments v2); distinct from an order or capture ID.
        scope authorization(id: AuthorizationId) {
            path "v2/payments/authorizations/{id}";

            /// Capture an authorization. Repeated only with a caller-supplied [`RequestId`]
            /// and identical bytes; a lost answer remains uncertain if not resolved.
            POST capture(
                body request: AuthorizationCaptureRequest,
                /// Unique ID for this logical capture, distinct from authorize/create IDs.
                request_id?: RequestId,
            ) -> Capture {
                path "capture";
                headers {
                    "paypal-request-id" = "{request_id}";
                    "prefer" = "return=representation";
                }
            }

            /// Release an authorization that has not been fully captured. Returns an
            /// acknowledgement (`()`), accepting either documented `200` or `204` success.
            /// PayPal's current reference documents request IDs for void, too.
            POST void(
                /// Unique ID for this logical void. Without it, sent exactly once.
                request_id?: RequestId,
            ) {
                path "void";
                headers { "paypal-request-id" = "{request_id}"; }
            }
        }

        /// One captured payment (Payments v2).
        scope capture(id: CaptureId) {
            path "v2/payments/captures/{id}";

            /// Show captured payment details.
            GET get() -> Capture;
        }
    }
}

impl PayPal {
    /// A sandbox client with the default [`Builder`] settings.
    pub fn sandbox(http: reqwest::Client, credentials: Credentials) -> Self {
        Self::builder(http, credentials)
            .build()
            .expect("the declared sandbox base is a valid URL")
    }

    /// A client whose base URL, repetition bounds and token clock can be chosen.
    pub fn builder(http: reqwest::Client, credentials: Credentials) -> Builder {
        Builder {
            http,
            credentials,
            base: None,
            reads: policy::default_reads(),
            writes: policy::default_writes(),
            margin: Duration::from_secs(60),
            clock: None,
        }
    }
}

type Clock = Box<dyn Fn() -> Instant + Send + Sync + 'static>;

/// Configures a [`PayPal`] client and its [`Session`].
pub struct Builder {
    http: reqwest::Client,
    credentials: Credentials,
    base: Option<String>,
    reads: concord_retry::Policy,
    writes: concord_retry::Policy,
    margin: Duration,
    clock: Option<Clock>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Builder")
            .field("base", &self.base)
            .field("reads", &self.reads)
            .field("writes", &self.writes)
            .field("margin", &self.margin)
            .finish_non_exhaustive()
    }
}

impl Builder {
    /// Replaces the sandbox base (`https://api-m.paypal.com` for live, a local fixture in tests).
    /// The token endpoint and every API request use this one origin; a request to any other
    /// origin is refused ([`Error::Destination`]).
    #[must_use]
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base = Some(url.into());
        self
    }

    /// Bounds for reads (Show order details, Show captured payment details). Default: 3
    /// attempts, backoff from 500 ms to 4 s, no overall deadline.
    #[must_use]
    pub fn reads(mut self, policy: concord_retry::Policy) -> Self {
        self.reads = policy;
        self
    }

    /// Bounds for a documented write that carries a [`RequestId`] (without one, a write is
    /// always sent once). Default: 2 attempts (PayPal recommends repeating a timed-out or `5xx`
    /// capture at least once with the same ID), backoff from 1 s to 4 s, no overall deadline.
    /// A deadline that cancels an attempt in flight makes the write [`Error::Uncertain`].
    #[must_use]
    pub fn writes(mut self, policy: concord_retry::Policy) -> Self {
        self.writes = policy;
        self
    }

    /// How long before `expires_in` runs out a cached token is replaced (default 60 s). A token
    /// whose lifetime is not longer than this margin is refused ([`TokenError::Unusable`]).
    #[must_use]
    pub fn token_refresh_margin(mut self, margin: Duration) -> Self {
        self.margin = margin;
        self
    }

    /// The monotonic clock the token cache measures lifetimes with (default `Instant::now`).
    #[must_use]
    pub fn token_clock(mut self, clock: impl Fn() -> Instant + Send + Sync + 'static) -> Self {
        self.clock = Some(Box::new(clock));
        self
    }

    /// # Errors
    ///
    /// [`Error::Concord`] (`Build`) if the base URL is not a valid `http`/`https` base.
    pub fn build(self) -> Result<PayPal, Error> {
        let base = self.base.unwrap_or_else(|| SANDBOX.to_owned());
        let session = Session::new(
            self.http.clone(),
            self.credentials,
            &base,
            self.reads,
            self.writes,
            self.margin,
            self.clock,
        )?;
        let api = PayPal::new(self.http, PayPalConfig { session });
        Ok(api.with_base_url(&base)?)
    }
}

/// A transport for this SDK: no transport-level retries (a write is sent exactly as often as
/// the session decides), no redirects (credentials and writes never follow a hop), a 30-second
/// timeout per request. A client of your own needs the same two restrictions for the failure
/// semantics to hold.
///
/// # Errors
///
/// reqwest's, if the client cannot be built.
pub fn http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use concord::reqwest::header::CONTENT_TYPE;

    fn api() -> PayPal {
        let credentials = Credentials::new("id", "secret").unwrap();
        PayPal::sandbox(reqwest::Client::new(), credentials)
    }

    #[test]
    fn declared_base_is_the_documented_sandbox() {
        let order = OrderId::new("5O190127TN364715T").unwrap();
        let prepared = api().order(order).get().prepare().unwrap();
        assert_eq!(
            prepared.request().url().as_str(),
            format!("{SANDBOX}/v2/checkout/orders/5O190127TN364715T")
        );
    }

    #[test]
    fn header_names_compose() {
        PayPal::validate_header_names().unwrap();
    }

    #[test]
    fn capture_body_is_the_empty_object() {
        let order = OrderId::new("ABC").unwrap();
        let prepared = api()
            .order(order)
            .capture(CaptureRequest::new())
            .prepare()
            .unwrap();
        let body = prepared.request().body().and_then(|b| b.as_bytes());
        assert_eq!(body, Some(&b"{}"[..]));
        assert_eq!(
            prepared.request().headers()[CONTENT_TYPE],
            "application/json"
        );
    }
}

/// Compiles the README examples without making provider requests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub struct ReadmeExamples;
