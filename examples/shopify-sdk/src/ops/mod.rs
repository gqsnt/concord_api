//! The named operations: one variables type per GraphQL document, implementing
//! [`Operation`](crate::graphql::Operation), with the typed `data` it returns.
//!
//! Every document selects only fields verified against the pinned `2026-10` reference (see
//! `PROVIDER.md`). Nullable fields that are selected are read with
//! [`nullable`](crate::graphql::nullable): `null` is `None`, an absent field is a decode error.
//! A field selected only on request (`@include(if: ..)`) is an
//! [`Included`](crate::graphql::Included) instead: absent there means "not requested".

pub mod bulk;
pub mod catalog;
pub mod inventory;
pub mod shop;

use serde::Serialize;

use crate::scalars::InvalidValue;

/// A connection page size (`first`), 1 through 250 (the documented maximum per request).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct PageSize(u16);

impl PageSize {
    /// The largest page Shopify returns.
    pub const MAX: Self = Self(250);

    /// A page size.
    ///
    /// # Errors
    ///
    /// [`InvalidValue`] outside 1–250.
    pub const fn new(size: u16) -> Result<Self, InvalidValue> {
        if size == 0 || size > 250 {
            return Err(InvalidValue("page size"));
        }
        Ok(Self(size))
    }

    /// The size.
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// Declares a GraphQL enum read leniently: known values as variants, any other as `Other`.
macro_rules! open_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $wire:literal,)* }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)*
            /// A value this SDK does not know (kept, not refused).
            Other(String),
        }

        impl $name {
            /// The wire spelling.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)*
                    Self::Other(value) => value,
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($wire => Self::$variant,)*
                    _ => Self::Other(value),
                })
            }
        }
    };
}

pub(crate) use open_enum;
