#![doc = include_str!("../README.md")]

mod error;
mod models;

use concord::Json;

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
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        format Json;

        /// Retrieves one country/year array, in country-then-year path order.
        ///
        /// `country_code` is the provider's ISO 3166-1 alpha-2 spelling (for example `AT`).
        /// Values are sent as supplied, encoded as one path segment; this SDK does not
        /// claim to validate provider coverage or year availability. Use `fetch()` to
        /// preserve metadata and classify HTTP 400 as an invalid request. Generated
        /// `send()` remains available and returns Concord's own error instead.
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
    /// Status, HTTP version and headers, captured before decoding the body.
    pub meta: ResponseMeta,
}

impl NagerHolidaysHolidays {
    /// Executes once and decodes the complete holiday array, retaining response metadata.
    ///
    /// No retry or pagination is performed. No partial vector is returned if body reading
    /// or decoding fails. Error responses stay unread in [`Error::concord`].
    ///
    /// # Errors
    ///
    /// [`ErrorKind::InvalidRequest`] for HTTP 400, [`ErrorKind::Http`] for another non-2xx
    /// status, or [`ErrorKind::Concord`] for build, transport, media-type or decode failures.
    pub async fn fetch(self) -> Result<HolidayResponse, Error> {
        let received = self.prepare()?.receive().await?;
        let meta = ResponseMeta::capture(&received.head());
        match received.decode().await {
            Ok(holidays) => Ok(HolidayResponse { holidays, meta }),
            Err(error) => Err(Error::with_meta(error, meta)),
        }
    }
}
