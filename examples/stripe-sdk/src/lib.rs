//! A bounded Stripe v1 SDK slice written with `concord::api!`.
//!
//! **Scope.** Customers (list, retrieve, create, update), subscriptions (list, retrieve, create,
//! update), invoices (list, retrieve) and their line items (list), products (list, retrieve,
//! create), prices (list, retrieve, create) and balance transactions (list, retrieve): 19
//! operations of Stripe API v1, pinned to version [`PINNED_API_VERSION`]. It is not a complete
//! Stripe SDK; `PROVIDER.md` lists what is deliberately unsupported.
//!
//! **Layers.**
//!
//! - [`Stripe`] — the generated client (the `api!` declaration below). Every operation is an
//!   ordinary Concord request value: setters, `prepare()`, `send()`, `into_parts()`.
//! - [`StripeContext`] — the configured context (secret key, API version, connected account)
//!   that produces the generated handles with consistent typed values, and
//!   [`AccountOverride`]/[`OnAccount`] for a per-request `Stripe-Account`.
//! - [`send()`] — sends any prepared operation once and keeps Stripe's response metadata
//!   (`Request-Id`, `Idempotent-Replayed`, rate-limit reason) and typed error documents.
//! - [`pages`] — cursor pagination (`starting_after`/`ending_before`, `has_more`): every list
//!   operation's `items()` / `pages()` (`concord-pagination`'s owned traversals: `Ok(None)`
//!   only at the end, a cap or a failed page is an error), and the [`pages::CursorList`] adapter
//!   for the backward direction, resuming and the pagination engine.
//! - [`form::StripeForm`] — Stripe's bracketed form encoding, a custom Concord format.
//!
//! ```
//! use concord_example_stripe_sdk::{ExpandPath, Limit, SecretKey, StripeContext};
//! use concord::reqwest;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let key = SecretKey::new("sk_test_example")?;
//! let stripe = StripeContext::new(reqwest::Client::new(), &key);
//! let invoices = stripe
//!     .api()
//!     .invoices()
//!     .list()
//!     .set_limit(Limit::new(25)?)
//!     .set_expand(vec![ExpandPath::new("data.customer")?]);
//! let prepared = invoices.prepare()?;
//! assert_eq!(
//!     prepared.request().url().as_str(),
//!     "https://api.stripe.com/v1/invoices?limit=25&expand%5B%5D=data.customer",
//! );
//! assert_eq!(prepared.request().headers()["stripe-version"], "2026-10-28.endive");
//! assert!(!format!("{prepared:?}").contains("sk_test_example"));
//! # Ok(()) }
//! ```

pub mod context;
pub mod expandable;
pub mod form;
pub mod ids;
pub mod models;
pub mod pages;
pub mod params;
pub mod send;
pub mod types;

use std::fmt;

use concord::Json;
use concord::reqwest::header::{AUTHORIZATION, HeaderValue};

pub use context::{AccountOverride, OnAccount, StripeContext};
pub use expandable::{Expandable, List, Object};
pub use form::{MetadataPatch, Patch, StripeForm};
pub use ids::{
    AccountId, BalanceTransactionId, CustomerId, IdError, InvoiceId, InvoiceLineItemId,
    PaymentMethodId, PriceId, ProductId, SubscriptionId, SubscriptionItemId, TaxRateId,
};
pub use models::{
    BalanceTransaction, CollectionMethod, Customer, Invoice, InvoiceLineItem, InvoiceStatus,
    PaymentMethod, Price, Product, SourceObject, Subscription, SubscriptionItem,
    SubscriptionStatus,
};
pub use send::{ApiError, ApiErrorType, Error, ProtocolError, Response, ResponseMeta, send};
pub use types::{
    ApiVersion, Currency, ExpandPath, IdempotencyKey, Limit, Metadata, PINNED_API_VERSION,
    SecretKey, Timestamp, ValueError,
};

use params::{
    CustomerCreate, CustomerUpdate, PriceCreate, ProductCreate, SubscriptionCreate,
    SubscriptionUpdate,
};

/// The `status` filter of `GET /v1/subscriptions` (adds `all` and `ended` to the statuses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubscriptionStatusFilter {
    /// `active`.
    Active,
    /// `all` (including canceled).
    All,
    /// `canceled`.
    Canceled,
    /// `ended`.
    Ended,
    /// `incomplete`.
    Incomplete,
    /// `incomplete_expired`.
    IncompleteExpired,
    /// `past_due`.
    PastDue,
    /// `paused`.
    Paused,
    /// `trialing`.
    Trialing,
    /// `unpaid`.
    Unpaid,
}

impl fmt::Display for SubscriptionStatusFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Active => "active",
            Self::All => "all",
            Self::Canceled => "canceled",
            Self::Ended => "ended",
            Self::Incomplete => "incomplete",
            Self::IncompleteExpired => "incomplete_expired",
            Self::PastDue => "past_due",
            Self::Paused => "paused",
            Self::Trialing => "trialing",
            Self::Unpaid => "unpaid",
        })
    }
}

/// The `type` filter of `GET /v1/prices`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PriceTypeFilter {
    /// `one_time`.
    OneTime,
    /// `recurring`.
    Recurring,
}

impl fmt::Display for PriceTypeFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OneTime => "one_time",
            Self::Recurring => "recurring",
        })
    }
}

impl fmt::Display for CollectionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ChargeAutomatically => "charge_automatically",
            Self::SendInvoice => "send_invoice",
        })
    }
}

impl fmt::Display for InvoiceStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Paid => "paid",
            Self::Uncollectible => "uncollectible",
            Self::Void => "void",
        })
    }
}

concord::api! {
    /// Stripe API v1, a bounded slice (see `PROVIDER.md`). Create it through [`StripeContext`],
    /// which builds the sensitive `Authorization` value from a [`SecretKey`] and keeps the typed
    /// version and account in step with the generated values.
    pub client Stripe(
        /// The complete `Authorization` value (`Bearer sk_...`), flagged sensitive.
        #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
        /// `Stripe-Version`, sent on every request so that the account's default version is
        /// never followed silently.
        header("stripe-version") version: String = PINNED_API_VERSION,
        /// `Stripe-Account`: the connected account requests act on. Absent: the platform's own
        /// account.
        #[sensitive] header("stripe-account") account?: HeaderValue,
    ) {
        base "https://api.stripe.com/v1";
        request StripeForm;
        response Json;

        /// Customers.
        scope customers {
            path "customers";

            /// `GET /v1/customers`: customers, most recent first.
            GET list(
                /// Exact, case-sensitive email filter.
                query email?: String,
                /// Created at or after (`created[gte]`).
                query("created[gte]") created_gte?: Timestamp,
                /// Created before (`created[lt]`).
                query("created[lt]") created_lt?: Timestamp,
                /// Page size, 1-100 (Stripe's default: 10).
                query limit?: Limit,
                /// Forward cursor: the last id of the previous page.
                query starting_after?: CustomerId,
                /// Backward cursor: the first id of the previous page.
                query ending_before?: CustomerId,
                /// `expand[]` entries, such as `data.test_clock` or
                /// `data.invoice_settings.default_payment_method`.
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                /// Per-request `Stripe-Account`: absent inherits the client's, `None` sends none
                /// (the platform), `Some(v)` sends `v`. Set it through [`OnAccount`].
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<Customer> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/customers/{id}`.
            GET retrieve(
                id: CustomerId,
                /// `expand[]` entries, such as `invoice_settings.default_payment_method`.
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Customer {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `POST /v1/customers`.
            POST create(
                body params: CustomerCreate,
                /// `Idempotency-Key`, supplied by the application (never generated here).
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Customer {
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }

            /// `POST /v1/customers/{id}`.
            POST update(
                id: CustomerId,
                body params: CustomerUpdate,
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Customer {
                path "{id}";
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }
        }

        /// Subscriptions.
        scope subscriptions {
            path "subscriptions";

            /// `GET /v1/subscriptions`: subscriptions, most recent first (Stripe's default
            /// status filter excludes canceled ones).
            GET list(
                query customer?: CustomerId,
                query price?: PriceId,
                query status?: SubscriptionStatusFilter,
                query collection_method?: CollectionMethod,
                query("created[gte]") created_gte?: Timestamp,
                query("created[lt]") created_lt?: Timestamp,
                query limit?: Limit,
                query starting_after?: SubscriptionId,
                query ending_before?: SubscriptionId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<Subscription> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/subscriptions/{id}`.
            GET retrieve(
                id: SubscriptionId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Subscription {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `POST /v1/subscriptions`.
            POST create(
                body params: SubscriptionCreate,
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Subscription {
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }

            /// `POST /v1/subscriptions/{id}`.
            POST update(
                id: SubscriptionId,
                body params: SubscriptionUpdate,
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Subscription {
                path "{id}";
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }
        }

        /// Invoices and their line items.
        scope invoices {
            path "invoices";

            /// `GET /v1/invoices`: invoices, most recently created first.
            GET list(
                query customer?: CustomerId,
                query subscription?: SubscriptionId,
                query status?: InvoiceStatus,
                query collection_method?: CollectionMethod,
                query("created[gte]") created_gte?: Timestamp,
                query("created[lt]") created_lt?: Timestamp,
                query limit?: Limit,
                query starting_after?: InvoiceId,
                query ending_before?: InvoiceId,
                /// Such as `data.customer`, `data.parent.subscription_details.subscription`.
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<Invoice> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/invoices/{id}`.
            GET retrieve(
                id: InvoiceId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Invoice {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/invoices/{id}/lines`: the invoice's line items.
            GET lines(
                id: InvoiceId,
                query limit?: Limit,
                query starting_after?: InvoiceLineItemId,
                query ending_before?: InvoiceLineItemId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<InvoiceLineItem> {
                path "{id}/lines";
                headers { override "stripe-account" = each account_override else inherit; }
            }
        }

        /// Products.
        scope products {
            path "products";

            /// `GET /v1/products`.
            GET list(
                query active?: bool,
                /// `ids[]`: only these products.
                query("ids[]") each ids: Vec<ProductId> = Vec::new(),
                query("created[gte]") created_gte?: Timestamp,
                query("created[lt]") created_lt?: Timestamp,
                query limit?: Limit,
                query starting_after?: ProductId,
                query ending_before?: ProductId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<Product> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/products/{id}`.
            GET retrieve(
                id: ProductId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Product {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `POST /v1/products`.
            POST create(
                body params: ProductCreate,
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Product {
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }
        }

        /// Prices.
        scope prices {
            path "prices";

            /// `GET /v1/prices`.
            GET list(
                query product?: ProductId,
                query active?: bool,
                query currency?: Currency,
                query("type") kind?: PriceTypeFilter,
                query("lookup_keys[]") each lookup_keys: Vec<String> = Vec::new(),
                query("created[gte]") created_gte?: Timestamp,
                query("created[lt]") created_lt?: Timestamp,
                query limit?: Limit,
                query starting_after?: PriceId,
                query ending_before?: PriceId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<Price> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/prices/{id}`.
            GET retrieve(
                id: PriceId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Price {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `POST /v1/prices`.
            POST create(
                body params: PriceCreate,
                #[sensitive] idempotency_key?: IdempotencyKey,
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> Price {
                headers {
                    "idempotency-key" = "{idempotency_key}";
                    override "stripe-account" = each account_override else inherit;
                }
            }
        }

        /// Balance transactions.
        scope balance_transactions {
            path "balance_transactions";

            /// `GET /v1/balance_transactions`: most recent first.
            GET list(
                /// Transaction type, such as `charge` or `payout`.
                query("type") kind?: String,
                query currency?: Currency,
                /// Only transactions paid out in this payout (`po_...`).
                query payout?: String,
                /// Only transactions related to this object.
                query source?: String,
                query("created[gte]") created_gte?: Timestamp,
                query("created[lt]") created_lt?: Timestamp,
                query limit?: Limit,
                query starting_after?: BalanceTransactionId,
                query ending_before?: BalanceTransactionId,
                /// Such as `data.source`.
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> List<BalanceTransaction> {
                headers { override "stripe-account" = each account_override else inherit; }
            }

            /// `GET /v1/balance_transactions/{id}`.
            GET retrieve(
                id: BalanceTransactionId,
                query("expand[]") each expand: Vec<ExpandPath> = Vec::new(),
                #[sensitive] account_override?: Option<HeaderValue>,
            ) -> BalanceTransaction {
                path "{id}";
                headers { override "stripe-account" = each account_override else inherit; }
            }
        }
    }
}
