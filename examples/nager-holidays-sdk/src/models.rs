//! The v4 response model; unknown object fields are ignored by Serde.

use serde::{Deserialize, Deserializer};

use crate::Date;

/// One holiday as reported by the v4 endpoint.
///
/// Unknown JSON fields are ignored without changing the known fields. A null
/// subdivision list remains `None` even when `national_holiday` is false; callers
/// should use the provider's flag for national filtering.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holiday {
    /// The provider's ISO date, parsed without a timezone using Chrono.
    pub date: Date,
    /// Common English name.
    pub name: String,
    /// Provider-supplied ISO 3166-1 alpha-2 country spelling.
    pub country_code: String,
    /// Whether the provider marks this as a national holiday.
    pub national_holiday: bool,
    /// Provider subdivision codes. JSON `null` or omission is `None`; `[]` is `Some(vec![])`.
    /// No relationship between this list and `national_holiday` is inferred.
    pub subdivision_codes: Option<Vec<String>>,
    /// Holiday classifications, including any unrecognized future values.
    pub holiday_types: Vec<HolidayType>,
}

/// The six documented classifications, plus a preserved future string value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HolidayType {
    /// Public holiday.
    Public,
    /// Bank holiday.
    Bank,
    /// School holiday.
    School,
    /// Authorities are closed.
    Authorities,
    /// Optional holiday.
    Optional,
    /// Observance.
    Observance,
    /// An unknown classification, retained exactly as received.
    Other(String),
}

impl<'de> Deserialize<'de> for HolidayType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "Public" => Self::Public,
            "Bank" => Self::Bank,
            "School" => Self::School,
            "Authorities" => Self::Authorities,
            "Optional" => Self::Optional,
            "Observance" => Self::Observance,
            other => Self::Other(other.to_owned()),
        })
    }
}
