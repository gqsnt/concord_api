#![doc = include_str!("../README.md")]

mod error;
mod models;

use concord::{Endpoint, Json, SendWith};

pub use chrono::NaiveDate as Date;
pub use error::{Error, ErrorKind, ResponseMeta};
pub use models::{Holiday, HolidayType};

/// The pinned provider contract, rechecked on 2026-11-06 (see `PROVENANCE.md`).
pub const API_VERSION: &str = "v4";

concord::api! {
    /// Client for the unauthenticated Nager.Holidays Community v4 holiday endpoint.
    ///
    /// `NagerHolidays::new(http)` uses the hosted origin. For loopback tests or another
    /// deployment, `with_base_url(origin)` replaces the origin; the `/api/v4` prefix is
    /// declared on the operation. No auth or context configuration record is required.
    /// `send()` goes through the SDK's send policy [`KeepHead`].
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        send KeepHead;
        format Json;

        /// Retrieves one country/year array, in country-then-year path order.
        ///
        /// `country_code` is the provider's ISO 3166-1 alpha-2 spelling (for example `AT`).
        /// Values are sent as supplied, encoded as one path segment; this SDK does not
        /// claim to validate provider coverage or year availability. `send()` returns a
        /// [`HolidayResponse`] with the response metadata, or this SDK's [`Error`], which
        /// classifies HTTP 400 as an invalid request ([`KeepHead`]). `prepare()` builds the
        /// native request; `prepare()?.send()` returns Concord's own error instead.
        GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
            path "api/v4/Holidays/{country_code}/{year}";
        }
    }
}

/// The complete finite result of one holiday request, together with its response head.
#[derive(Clone, Debug)]
pub struct HolidayResponse {
    /// All decoded entries, in the provider's order; no calendar interpretation is applied.
    pub holidays: Vec<Holiday>,
    /// Status, HTTP version, headers and operation, captured before decoding the body.
    pub meta: ResponseMeta,
}

/// The send policy of [`NagerHolidays`] (`send KeepHead;`): `holidays(..).send()` executes once
/// and decodes the complete holiday array, retaining response metadata, and returns
/// `Result<HolidayResponse, Error>`.
///
/// No retry or pagination is performed. No partial vector is returned if body reading or
/// decoding fails. Error responses stay unread in [`Error::concord`].
///
/// Errors: [`ErrorKind::InvalidRequest`] for HTTP 400, [`ErrorKind::Http`] for another non-2xx
/// status, or [`ErrorKind::Concord`] for build, transport, media-type or decode failures.
#[derive(Default)]
pub struct KeepHead;

impl SendWith<Vec<Holiday>> for KeepHead {
    type Output = Result<HolidayResponse, Error>;

    async fn send<E: Endpoint<Vec<Holiday>> + Send>(&self, endpoint: E) -> Self::Output {
        // A failure after the head arrived keeps it in the Concord error (`Error::head_meta`).
        let received = endpoint.prepare()?.receive().await?;
        let (holidays, meta) = received.decode_with_meta().await?;
        Ok(HolidayResponse { holidays, meta })
    }
}
