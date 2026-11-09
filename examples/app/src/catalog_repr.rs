//! Catalog's representation of the application's domain values.
//!
//! Catalog writes an account as its decimal number (`42`) and a time as whole-second RFC 3339
//! UTC (`2026-01-15T10:00:00Z`). Both rules are Catalog's, so they live here and nowhere else.
//! An instant Catalog cannot represent exactly is refused with a [`CatalogReprError`]; nothing is
//! rounded or truncated.
//!
//! [`activity`] is the boundary the rest of the application uses: a synchronous, fallible
//! conversion that returns the generated [`CatalogActivity`] operation. The caller keeps its
//! setters, `prepare()` and `send()`.

use concord_example_sdk::{Catalog, CatalogActivity};

use crate::domain::{AccountId, Moment};

/// A domain value Catalog cannot represent exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, concord::derive::Error)]
#[display(with = display_catalog_repr_error)]
pub enum CatalogReprError {
    /// Catalog's `since` has whole seconds only, and the instant has a fractional part.
    FractionalSeconds {
        /// The nanoseconds past the whole second.
        subsec_nanos: u32,
    },
    /// The instant falls outside the four-digit years `0000` through `9999` that the
    /// `YYYY-MM-DDTHH:MM:SSZ` form can write.
    YearOutOfRange {
        /// The instant's Unix seconds.
        unix_seconds: i64,
    },
}

fn display_catalog_repr_error(
    value: &CatalogReprError,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    match *value {
        CatalogReprError::FractionalSeconds { subsec_nanos } => write!(
            f,
            "Catalog `since` takes whole seconds; the instant is {subsec_nanos} ns past its second"
        ),
        CatalogReprError::YearOutOfRange { unix_seconds } => write!(
            f,
            "Catalog `since` takes years 0000 through 9999; Unix second {unix_seconds} is outside them"
        ),
    }
}

/// `0000-01-01T00:00:00Z`.
const FIRST_SECOND: i64 = -62_167_219_200;
/// `9999-12-31T23:59:59Z`.
const LAST_SECOND: i64 = 253_402_300_799;

/// Catalog's account text: the decimal number.
pub fn account(id: AccountId) -> String {
    id.get().to_string()
}

/// Catalog's `since` text: whole-second RFC 3339 UTC, `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Checks, in this order: the instant has no fractional second
/// ([`FractionalSeconds`](CatalogReprError::FractionalSeconds)); its year is `0000` through
/// `9999` ([`YearOutOfRange`](CatalogReprError::YearOutOfRange)). Instants before 1970 are valid
/// (`-1` is `1969-12-31T23:59:59Z`).
pub fn since(moment: Moment) -> Result<String, CatalogReprError> {
    if moment.subsec_nanos() != 0 {
        return Err(CatalogReprError::FractionalSeconds {
            subsec_nanos: moment.subsec_nanos(),
        });
    }
    let seconds = moment.unix_seconds();
    if !(FIRST_SECOND..=LAST_SECOND).contains(&seconds) {
        return Err(CatalogReprError::YearOutOfRange {
            unix_seconds: seconds,
        });
    }
    let (year, month, day) = civil_from_days(seconds.div_euclid(86_400));
    let in_day = seconds.rem_euclid(86_400);
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        in_day / 3_600,
        in_day % 3_600 / 60,
        in_day % 60
    ))
}

/// The proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's
/// `civil_from_days`). Only called within the four-digit-year range, so nothing overflows.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365], from March 1st
    let mp = (5 * doy + 2) / 153; // [0, 11], March is 0
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `GET accounts/{account}/activity` for a domain account, from `since` when given.
///
/// Converts synchronously and returns the generated operation; nothing is built or sent. A
/// `since` Catalog cannot represent is refused here, before any request exists.
pub fn activity(
    api: &Catalog,
    id: AccountId,
    since: Option<Moment>,
) -> Result<CatalogActivity, CatalogReprError> {
    let since = since.map(self::since).transpose()?;
    Ok(api.activity(account(id)).maybe_since(since))
}
