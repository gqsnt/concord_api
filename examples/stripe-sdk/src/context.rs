//! The configured Stripe context and per-request account overrides.
//!
//! A [`StripeContext`] is configured once — the `reqwest::Client`, the secret key, the API
//! version and optionally a connected account — and hands out the generated [`Stripe`] handle
//! built from exactly those values. Like Concord's handles it is an immutable snapshot: a setter
//! consumes it and returns a new one, and cloning is a reference-count increment. Two contexts
//! for two accounts share nothing but the `reqwest::Client`.
//!
//! The context keeps the typed values ([`ApiVersion`], [`AccountId`]) that reports and
//! checkpoints record, alongside the generated client whose header values come from them. The
//! secret key is turned into the `Authorization` value once, flagged sensitive, and not kept.
//!
//! # Per-request account
//!
//! Every operation of the declaration has an `account_override` value with three states, set
//! through [`OnAccount::on_account`]:
//!
//! | [`AccountOverride`] | `Stripe-Account` sent |
//! |---|---|
//! | `Inherit` | the context's account, if any |
//! | `Platform` | none: the platform's own account, whatever the context says |
//! | `Account(id)` | `id`, for this request only |

use std::fmt;
use std::sync::Arc;

use concord::reqwest::{self, header::HeaderValue};

use crate::ids::AccountId;
use crate::types::{ApiVersion, SecretKey};
use crate::{
    Stripe, StripeBalanceTransactionsList, StripeBalanceTransactionsRetrieve, StripeConfig,
    StripeCustomersCreate, StripeCustomersList, StripeCustomersRetrieve, StripeCustomersUpdate,
    StripeInvoicesLines, StripeInvoicesList, StripeInvoicesRetrieve, StripePricesCreate,
    StripePricesList, StripePricesRetrieve, StripeProductsCreate, StripeProductsList,
    StripeProductsRetrieve, StripeSubscriptionsCreate, StripeSubscriptionsList,
    StripeSubscriptionsRetrieve, StripeSubscriptionsUpdate,
};

/// The base URL declared in the `Stripe` declaration.
pub const DECLARED_BASE: &str = "https://api.stripe.com/v1";

/// Sends `authorization: Bearer <key>` from the supplied `SecretKey`, `stripe-version`
/// from the context's `ApiVersion::as_str()` (initially pinned), and `stripe-account`
/// from the selected `AccountId`, when present. `OnAccount` documents per-request overrides.
///
/// A configured Stripe context. See the [module documentation](self).
#[derive(Clone)]
pub struct StripeContext {
    inner: Arc<Inner>,
}

#[derive(Clone)]
struct Inner {
    api: Stripe,
    base: String,
    version: ApiVersion,
    account: Option<AccountId>,
}

impl StripeContext {
    /// A context of `http` and `key`, against `https://api.stripe.com/v1`, with the pinned API
    /// version and no connected account (the platform's own account).
    pub fn new(http: reqwest::Client, key: &SecretKey) -> Self {
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", key.expose()))
            .expect("a validated secret key is visible ASCII");
        authorization.set_sensitive(true);
        let version = ApiVersion::pinned();
        let api = Stripe::new(http, StripeConfig { authorization }).set_version(version.as_str());
        Self {
            inner: Arc::new(Inner {
                api,
                base: DECLARED_BASE.to_owned(),
                version,
                account: None,
            }),
        }
    }

    /// Replaces the base URL (a local test server, a proxy path). Validated by Concord's own
    /// [`Stripe::with_base_url`].
    ///
    /// # Errors
    ///
    /// Concord's `Build` error for a URL it does not accept as a base.
    pub fn with_base_url(mut self, url: impl AsRef<str>) -> Result<Self, concord::Error> {
        let inner = Arc::make_mut(&mut self.inner);
        inner.api = inner.api.clone().with_base_url(url.as_ref())?;
        inner.base = url.as_ref().to_owned();
        Ok(self)
    }

    /// Sends `version` as `Stripe-Version` instead of the pinned one.
    pub fn with_version(mut self, version: ApiVersion) -> Self {
        let inner = Arc::make_mut(&mut self.inner);
        inner.api = inner.api.clone().set_version(version.as_str());
        inner.version = version;
        self
    }

    /// Acts on `account` (`Some`: every request sends `Stripe-Account`) or on the platform's own
    /// account (`None`).
    pub fn with_account(mut self, account: Option<AccountId>) -> Self {
        let inner = Arc::make_mut(&mut self.inner);
        inner.api = inner
            .api
            .clone()
            .maybe_account(account.as_ref().map(account_header));
        inner.account = account;
        self
    }

    /// The generated client handle, configured with this context's values.
    pub fn api(&self) -> &Stripe {
        &self.inner.api
    }

    /// The API version sent.
    pub fn version(&self) -> &ApiVersion {
        &self.inner.version
    }

    /// The connected account, if any.
    pub fn account(&self) -> Option<&AccountId> {
        self.inner.account.as_ref()
    }

    /// The base URL.
    pub fn base_url(&self) -> &str {
        &self.inner.base
    }
}

impl fmt::Debug for StripeContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StripeContext")
            .field("base", &self.inner.base)
            .field("version", &self.inner.version.as_str())
            .field("key", &format_args!("<redacted>"))
            .field(
                "account",
                &self
                    .inner
                    .account
                    .as_ref()
                    .map(|_| format_args!("<redacted>")),
            )
            .finish()
    }
}

/// `Stripe-Account` value for `account`, flagged sensitive.
pub(crate) fn account_header(account: &AccountId) -> HeaderValue {
    let mut value =
        HeaderValue::from_str(account.as_str()).expect("an account id is visible ASCII");
    value.set_sensitive(true);
    value
}

/// Which account one request acts on. See the [module documentation](self).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AccountOverride {
    /// The context's account (the default).
    #[default]
    Inherit,
    /// The platform's own account: no `Stripe-Account`, even if the context has one.
    Platform,
    /// This connected account, for this request only.
    Account(AccountId),
}

/// Per-request `Stripe-Account` for every generated operation of the declaration.
pub trait OnAccount: Sized {
    /// Applies `account` to this request (see [`AccountOverride`]).
    #[must_use]
    fn on_account(self, account: &AccountOverride) -> Self;
}

macro_rules! on_account {
    ($($operation:ty),* $(,)?) => {
        $(impl OnAccount for $operation {
            fn on_account(self, account: &AccountOverride) -> Self {
                match account {
                    AccountOverride::Inherit => self.maybe_account_override(None),
                    AccountOverride::Platform => self.set_account_override(None::<HeaderValue>),
                    AccountOverride::Account(id) => {
                        self.set_account_override(Some(account_header(id)))
                    }
                }
            }
        })*
    };
}

on_account!(
    StripeCustomersList,
    StripeCustomersRetrieve,
    StripeCustomersCreate,
    StripeCustomersUpdate,
    StripeSubscriptionsList,
    StripeSubscriptionsRetrieve,
    StripeSubscriptionsCreate,
    StripeSubscriptionsUpdate,
    StripeInvoicesList,
    StripeInvoicesRetrieve,
    StripeInvoicesLines,
    StripeProductsList,
    StripeProductsRetrieve,
    StripeProductsCreate,
    StripePricesList,
    StripePricesRetrieve,
    StripePricesCreate,
    StripeBalanceTransactionsList,
    StripeBalanceTransactionsRetrieve,
);
