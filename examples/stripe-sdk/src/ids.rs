//! Typed Stripe object identifiers.
//!
//! Each Stripe object family has its own identifier type, so a customer id cannot be passed where
//! an invoice id is expected. An identifier is path data (`customers/{id}`) and a list cursor
//! (`starting_after={id}`), written with its `Display` text.
//!
//! # Validation
//!
//! - **Constructed by the application** ([`CustomerId::new`], `str::parse`): the value must be
//!   1–255 visible ASCII characters without `/`, `?`, `#`, `%` or whitespace, and must start with
//!   the family's documented prefix (`cus_`, `in_`, ...). [`ProductId`] has no prefix rule: Stripe
//!   lets a product be created with a caller-chosen id.
//! - **Received from Stripe** (deserialized): the same length and character rules, but no
//!   prefix. The provider is authoritative for its own identifiers (preview invoices, for
//!   example, are documented with an `upcoming_in` prefix), so the SDK does not reject a response
//!   for a prefix it did not choose; the character rule still guarantees that every id can be a
//!   path segment, a query value and a header value.
//!
//! [`AccountId`] is an account identifier: its `Debug` is redacted (`AccountId(<redacted>)`), and
//! it reaches the wire only as the `Stripe-Account` header, which the declaration marks
//! sensitive.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An identifier that cannot be constructed from the given text. `Display` names the expected
/// family and the rule, never the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdError {
    family: &'static str,
    reason: IdErrorReason,
}

/// Why an identifier was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdErrorReason {
    /// The text is empty or longer than 255 characters.
    Length,
    /// The text contains whitespace, a control character, non-ASCII, `/`, `?`, `#` or `%`.
    Character,
    /// The text does not start with the family's prefix.
    Prefix(&'static str),
}

impl IdError {
    /// The identifier family, such as `"customer"`.
    pub fn family(&self) -> &'static str {
        self.family
    }

    /// Why the text was refused.
    pub fn reason(&self) -> IdErrorReason {
        self.reason
    }
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.reason {
            IdErrorReason::Length => write!(f, "{} id must be 1-255 characters", self.family),
            IdErrorReason::Character => write!(
                f,
                "{} id must be visible ASCII without `/`, `?`, `#` or `%`",
                self.family
            ),
            IdErrorReason::Prefix(prefix) => {
                write!(f, "{} id must start with `{prefix}`", self.family)
            }
        }
    }
}

impl std::error::Error for IdError {}

fn check(text: &str, family: &'static str, prefix: Option<&'static str>) -> Result<(), IdError> {
    if text.is_empty() || text.len() > 255 {
        return Err(IdError {
            family,
            reason: IdErrorReason::Length,
        });
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'?' | b'#' | b'%'))
    {
        return Err(IdError {
            family,
            reason: IdErrorReason::Character,
        });
    }
    if let Some(prefix) = prefix
        && !text.starts_with(prefix)
    {
        return Err(IdError {
            family,
            reason: IdErrorReason::Prefix(prefix),
        });
    }
    Ok(())
}

macro_rules! stripe_id {
    ($(#[$doc:meta])* $name:ident, $family:literal, $prefix:expr) => {
        $(#[$doc])*
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// The family's documented prefix, if it has one.
            pub const PREFIX: Option<&'static str> = $prefix;

            /// Validates `text` as an identifier of this family (see the
            /// [module documentation](self)).
            ///
            /// # Errors
            ///
            /// [`IdError`] when the text is empty, too long, has a forbidden character or lacks
            /// the prefix.
            pub fn new(text: impl Into<String>) -> Result<Self, IdError> {
                let text = text.into();
                check(&text, $family, Self::PREFIX)?;
                Ok(Self(text))
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;
            fn from_str(text: &str) -> Result<Self, IdError> {
                Self::new(text)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                check(&text, $family, None).map_err(serde::de::Error::custom)?;
                Ok(Self(text))
            }
        }
    };
}

macro_rules! public_debug {
    ($name:ident) => {
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_tuple(stringify!($name)).field(&self.0).finish()
            }
        }
    };
}

stripe_id!(
    /// A customer id, `cus_...`.
    CustomerId, "customer", Some("cus_")
);
public_debug!(CustomerId);
stripe_id!(
    /// A subscription id, `sub_...`.
    SubscriptionId, "subscription", Some("sub_")
);
public_debug!(SubscriptionId);
stripe_id!(
    /// A subscription item id, `si_...`.
    SubscriptionItemId, "subscription item", Some("si_")
);
public_debug!(SubscriptionItemId);
stripe_id!(
    /// An invoice id, `in_...`.
    InvoiceId, "invoice", Some("in_")
);
public_debug!(InvoiceId);
stripe_id!(
    /// An invoice line item id, `il_...` (the documentation's examples also show `il_tmp_...`).
    InvoiceLineItemId, "invoice line item", Some("il_")
);
public_debug!(InvoiceLineItemId);
stripe_id!(
    /// A product id. Usually `prod_...`, but a product may be created with a caller-chosen id,
    /// so no prefix is required.
    ProductId, "product", None
);
public_debug!(ProductId);
stripe_id!(
    /// A price id, `price_...`.
    PriceId, "price", Some("price_")
);
public_debug!(PriceId);
stripe_id!(
    /// A balance transaction id, `txn_...`.
    BalanceTransactionId, "balance transaction", Some("txn_")
);
public_debug!(BalanceTransactionId);
stripe_id!(
    /// A tax rate id, `txr_...`.
    TaxRateId, "tax rate", Some("txr_")
);
public_debug!(TaxRateId);
stripe_id!(
    /// A payment method id, `pm_...`. Legacy `card_`/`src_` ids are not accepted where Stripe
    /// expects a PaymentMethod id (see `PROVIDER.md`), so the prefix is required here.
    PaymentMethodId, "payment method", Some("pm_")
);
public_debug!(PaymentMethodId);
stripe_id!(
    /// A connected account id, `acct_...`. `Debug` is redacted; the text is reachable through
    /// [`AccountId::as_str`] and `Display`.
    AccountId, "account", Some("acct_")
);

impl fmt::Debug for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccountId(<redacted>)")
    }
}
