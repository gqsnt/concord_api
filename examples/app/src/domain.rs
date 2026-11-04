//! The application's domain values.
//!
//! They stand in for types from an independent domain crate: they model the application's
//! meaning (an account, an instant) and deliberately implement no service's wire encoding. There
//! is no `Display` that happens to print what Catalog or Ledger expects; each service's text is
//! produced by its own conversion ([`catalog_repr`](crate::catalog_repr),
//! [`ledger_repr`](crate::ledger_repr)), which may refuse a value it cannot represent exactly.

use std::fmt;

/// An account identifier: a nonnegative integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountId(u64);

impl AccountId {
    /// The account with this number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The account number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// An instant: whole seconds since the Unix epoch (negative before 1970) plus nanoseconds past
/// that second.
///
/// The nanoseconds always count forward from `unix_seconds`, so half a second before the epoch
/// is `(-1, 500_000_000)`. The derived ordering is chronological.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Moment {
    unix_seconds: i64,
    subsec_nanos: u32,
}

impl Moment {
    /// The instant `subsec_nanos` nanoseconds after Unix second `unix_seconds`.
    ///
    /// Fails when `subsec_nanos` is not below one second (`1_000_000_000`).
    pub const fn new(unix_seconds: i64, subsec_nanos: u32) -> Result<Self, NanosOutOfRange> {
        if subsec_nanos >= 1_000_000_000 {
            return Err(NanosOutOfRange { subsec_nanos });
        }
        Ok(Self {
            unix_seconds,
            subsec_nanos,
        })
    }

    /// The instant at the start of Unix second `unix_seconds`.
    pub const fn from_unix_seconds(unix_seconds: i64) -> Self {
        Self {
            unix_seconds,
            subsec_nanos: 0,
        }
    }

    /// Whole seconds since the Unix epoch (the second this instant falls in).
    pub const fn unix_seconds(self) -> i64 {
        self.unix_seconds
    }

    /// Nanoseconds past [`unix_seconds`](Self::unix_seconds), below `1_000_000_000`.
    pub const fn subsec_nanos(self) -> u32 {
        self.subsec_nanos
    }
}

/// [`Moment::new`] was given a subsecond part of one second or more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NanosOutOfRange {
    /// The rejected value.
    pub subsec_nanos: u32,
}

impl fmt::Display for NanosOutOfRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "subsecond nanoseconds must be below 1000000000, got {}",
            self.subsec_nanos
        )
    }
}

impl std::error::Error for NanosOutOfRange {}
