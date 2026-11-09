//! Ledger's representation of the application's domain values.
//!
//! Ledger writes an account as `acct-` plus lowercase hexadecimal, at least four digits
//! (`acct-002a` for 42), and a time as a nonnegative count of milliseconds since the Unix epoch
//! (`1768471200000`). Both rules are Ledger's, so they live here and nowhere else. An instant
//! Ledger cannot represent exactly is refused with a [`LedgerReprError`]; nothing is rounded,
//! wrapped or truncated.
//!
//! [`entries`] is the boundary the rest of the application uses: a synchronous, fallible
//! conversion that returns the generated [`LedgerEntries`] operation. The caller keeps its
//! setters (`set_page_size`, `maybe_page_token`), `prepare()` and `send()`.

use concord_example_sdk::{Ledger, LedgerEntries};

use crate::domain::{AccountId, Moment};

/// A domain value Ledger cannot represent exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, concord::derive::Error)]
#[display(with = display_ledger_repr_error)]
pub enum LedgerReprError {
    /// Ledger's `from_ms` counts whole milliseconds, and the instant has a finer part.
    SubMillisecond {
        /// The nanoseconds past the whole second.
        subsec_nanos: u32,
    },
    /// Ledger's `from_ms` is nonnegative, and the instant is before the Unix epoch.
    BeforeEpoch {
        /// The instant's Unix seconds.
        unix_seconds: i64,
    },
    /// The instant's millisecond count does not fit Ledger's unsigned 64-bit `from_ms`.
    OutOfRange {
        /// The instant's Unix seconds.
        unix_seconds: i64,
    },
}

fn display_ledger_repr_error(
    value: &LedgerReprError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match *value {
        LedgerReprError::SubMillisecond { subsec_nanos } => write!(
            f,
            "Ledger `from_ms` takes whole milliseconds; the instant is {subsec_nanos} ns past its second"
        ),
        LedgerReprError::BeforeEpoch { unix_seconds } => write!(
            f,
            "Ledger `from_ms` takes instants from the Unix epoch on; Unix second {unix_seconds} is before it"
        ),
        LedgerReprError::OutOfRange { unix_seconds } => write!(
            f,
            "Ledger `from_ms` is an unsigned 64-bit millisecond count; Unix second {unix_seconds} does not fit"
        ),
    }
}

/// Ledger's account text: `acct-` plus lowercase hexadecimal, zero-padded to four digits.
pub fn account(id: AccountId) -> String {
    format!("acct-{:04x}", id.get())
}

/// Ledger's millisecond count of an instant.
///
/// Checks, in this order: the instant is a whole millisecond
/// ([`SubMillisecond`](LedgerReprError::SubMillisecond)); it is not before the epoch
/// ([`BeforeEpoch`](LedgerReprError::BeforeEpoch)); its count fits a `u64`
/// ([`OutOfRange`](LedgerReprError::OutOfRange)).
pub fn millis(moment: Moment) -> Result<u64, LedgerReprError> {
    let nanos = moment.subsec_nanos();
    if !nanos.is_multiple_of(1_000_000) {
        return Err(LedgerReprError::SubMillisecond {
            subsec_nanos: nanos,
        });
    }
    let unix_seconds = moment.unix_seconds();
    let seconds =
        u64::try_from(unix_seconds).map_err(|_| LedgerReprError::BeforeEpoch { unix_seconds })?;
    seconds
        .checked_mul(1_000)
        .and_then(|ms| ms.checked_add(u64::from(nanos / 1_000_000)))
        .ok_or(LedgerReprError::OutOfRange { unix_seconds })
}

/// Ledger's `from_ms` text: [`millis`] in decimal.
pub fn from_ms(moment: Moment) -> Result<String, LedgerReprError> {
    millis(moment).map(|ms| ms.to_string())
}

/// `GET entries` for a domain account, from `from` when given.
///
/// Converts synchronously and returns the generated operation; nothing is built or sent. A
/// `from` Ledger cannot represent is refused here, before any request exists.
pub fn entries(
    api: &Ledger,
    id: AccountId,
    from: Option<Moment>,
) -> Result<LedgerEntries, LedgerReprError> {
    let from = from.map(from_ms).transpose()?;
    Ok(api.entries(account(id)).maybe_from_ms(from))
}
