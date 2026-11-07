//! Provider models: the `data.objects` / `data.meta` envelope and the country record.
//!
//! Every country field is optional: a response-field allowlist returns only the selected
//! properties, and a selected property a record lacks is simply absent. Neither is an error.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Number, Value};

use crate::error::ContractError;

/// One page of country records: `data.objects` with its `data.meta`, the shape every Countries
/// API endpoint of this crate returns (also a code lookup: a single-record page).
///
/// A page without records is an ordinary result (`countries` empty, `meta.total` 0 for a
/// search without match; an offset past the end also gives an empty page). Nothing is
/// requested again: [`next_offset`](Self::next_offset) and [`crate::pages`] continue only when
/// the provider reports more records.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(from = "Envelope")]
#[non_exhaustive]
pub struct CountryPage {
    /// `data.objects`, in the provider's order (ascending `names.common`).
    pub countries: Vec<Country>,
    /// `data.meta`, the slice description.
    pub meta: PageMeta,
    /// `data._demo`, present when the request used the provider's demo key: the records are a
    /// sample, not the dataset. Its shape is not documented, so it is kept as JSON.
    pub demo: Option<Value>,
}

#[derive(Deserialize)]
struct Envelope {
    data: Data,
}

#[derive(Deserialize)]
struct Data {
    objects: Vec<Country>,
    meta: PageMeta,
    #[serde(rename = "_demo", default)]
    demo: Option<Value>,
}

impl From<Envelope> for CountryPage {
    fn from(envelope: Envelope) -> Self {
        let Data {
            objects,
            meta,
            demo,
        } = envelope.data;
        Self {
            countries: objects,
            meta,
            demo,
        }
    }
}

/// `data.meta`: what slice of the matched set a page is.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct PageMeta {
    /// Matched records after filters.
    pub total: u64,
    /// Records in this page.
    pub count: u64,
    /// The page size the provider applied.
    pub limit: u64,
    /// Records skipped before this page.
    pub offset: u64,
    /// Whether records exist past this page.
    pub more: bool,
    /// The provider's request identifier, when sent.
    #[serde(default)]
    pub request_id: Option<String>,
}

impl CountryPage {
    /// The offset of the next page, `None` when this page ends the matched set.
    ///
    /// The page must be consistent: `count` equals the records received, at most `limit`
    /// records, the page ends at or before `total`, and `more` agrees with that arithmetic
    /// (checked by [`concord_pagination::next_offset`]). An empty page at or past `total` is the
    /// documented out-of-range answer and ends the set.
    ///
    /// # Errors
    ///
    /// [`ContractError`] when the page contradicts itself.
    pub fn next_offset(&self) -> Result<Option<u64>, ContractError> {
        let meta = &self.meta;
        let len = self.countries.len() as u64;
        if meta.count != len {
            return Err(ContractError::CountMismatch);
        }
        let next = if len == 0 && meta.offset >= meta.total {
            None
        } else {
            concord_pagination::next_offset(meta.offset, meta.limit, len, Some(meta.total))
                .map_err(ContractError::Offset)?
        };
        if next.is_some() != meta.more {
            return Err(ContractError::MoreMismatch);
        }
        Ok(next)
    }

    /// The one record of a lookup expected to be unique (an ISO code): `None` when nothing
    /// matched.
    ///
    /// # Errors
    ///
    /// [`ContractError::NotUnique`] when the provider reports more than one match (no record is
    /// chosen for the caller), [`ContractError::CountMismatch`] when `total` and the records
    /// received disagree (for example an offset that skipped the match).
    pub fn single(self) -> Result<Option<Country>, ContractError> {
        let len = self.countries.len() as u64;
        if self.meta.total > 1 || len > 1 {
            return Err(ContractError::NotUnique {
                total: self.meta.total.max(len),
            });
        }
        if self.meta.total != len || self.meta.count != len {
            return Err(ContractError::CountMismatch);
        }
        Ok(self.countries.into_iter().next())
    }
}

/// One country record. Only the properties a request selected (all of them without
/// `response_fields`) are present; the typed fields are the documented properties this crate
/// names, and every other top-level property the provider sent is kept unchanged in
/// [`other`](Self::other). Unknown keys inside the typed groups are ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Country {
    /// `names`.
    pub names: Option<Names>,
    /// `codes`.
    pub codes: Option<Codes>,
    /// `flag`.
    pub flag: Option<Flag>,
    /// `capitals`.
    pub capitals: Option<Vec<Capital>>,
    /// `region`.
    pub region: Option<String>,
    /// `subregion`.
    pub subregion: Option<String>,
    /// `continents`.
    pub continents: Option<Vec<String>>,
    /// `landlocked`.
    pub landlocked: Option<bool>,
    /// `borders`: ISO alpha-3 codes of countries sharing a land border.
    pub borders: Option<Vec<String>>,
    /// `population` (dynamic): a JSON number kept exactly as sent.
    pub population: Option<Number>,
    /// `memberships`: organisation name (`eu`, `un`, `nato`, ...) to membership.
    pub memberships: Option<BTreeMap<String, bool>>,
    /// `leaders` (premium): on a free plan the provider puts a notice in its place, which is
    /// [`Premium::Withheld`], not a failure. The leader objects are not documented field by
    /// field, so they are kept as JSON.
    pub leaders: Option<Premium<Vec<Value>>>,
    /// Every other top-level property, as sent (`cars`, `date`, `currencies`, `languages`, ...).
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Country {
    /// `names.common`, when selected.
    pub fn common_name(&self) -> Option<&str> {
        self.names.as_ref()?.common.as_deref()
    }

    /// `codes.alpha_2`, when selected.
    pub fn alpha_2(&self) -> Option<&str> {
        self.codes.as_ref()?.alpha_2.as_deref()
    }

    /// `codes.alpha_3`, when selected.
    pub fn alpha_3(&self) -> Option<&str> {
        self.codes.as_ref()?.alpha_3.as_deref()
    }
}

/// `names`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Names {
    /// `names.common`.
    pub common: Option<String>,
    /// `names.official`.
    pub official: Option<String>,
    /// `names.alternates`.
    pub alternates: Option<Vec<String>>,
    /// `names.native`, keyed by ISO 639-3 language code.
    pub native: Option<BTreeMap<String, NameForms>>,
    /// `names.translations`, keyed by ISO 639-3 language code.
    pub translations: Option<BTreeMap<String, NameForms>>,
}

/// The common and official form of a name in one language.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct NameForms {
    /// Common form.
    pub common: Option<String>,
    /// Official form.
    pub official: Option<String>,
}

/// `codes`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct Codes {
    /// `codes.alpha_2`.
    pub alpha_2: Option<String>,
    /// `codes.alpha_3`.
    pub alpha_3: Option<String>,
    /// `codes.ccn3`.
    pub ccn3: Option<String>,
    /// `codes.fips`.
    pub fips: Option<String>,
    /// `codes.gec`.
    pub gec: Option<String>,
    /// `codes.cioc`.
    pub cioc: Option<String>,
    /// `codes.fifa`.
    pub fifa: Option<String>,
}

/// `flag`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Flag {
    /// `flag.emoji`.
    pub emoji: Option<String>,
    /// `flag.unicode`.
    pub unicode: Option<String>,
    /// `flag.html_entity`.
    pub html_entity: Option<String>,
    /// `flag.url_png`.
    pub url_png: Option<String>,
    /// `flag.url_svg`.
    pub url_svg: Option<String>,
    /// `flag.description` (empty when the provider has none).
    pub description: Option<String>,
    /// `flag.colors`, as JSON.
    pub colors: Option<Value>,
}

/// One entry of `capitals`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[non_exhaustive]
pub struct Capital {
    /// The capital's name.
    pub name: Option<String>,
    /// Its coordinates, as JSON (not documented field by field).
    pub coordinates: Option<Value>,
    /// Role flags (`primary`, `constitutional`, `administrative`, ...).
    pub attributes: Option<BTreeMap<String, bool>>,
}

/// A premium property: its value, or the notice a free plan receives in its place.
#[derive(Clone, Debug, PartialEq)]
pub enum Premium<T> {
    /// The value (paid plans).
    Available(T),
    /// The provider's in-place notice (`[{"message": ".."}]`), kept verbatim.
    Withheld {
        /// The notice text. Untrusted provider text.
        message: String,
    },
}

impl<'de> Deserialize<'de> for Premium<Vec<Value>> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<Value>::deserialize(deserializer)?;
        if let [Value::Object(notice)] = values.as_slice()
            && notice.len() == 1
            && let Some(Value::String(message)) = notice.get("message")
        {
            return Ok(Self::Withheld {
                message: message.clone(),
            });
        }
        Ok(Self::Available(values))
    }
}
