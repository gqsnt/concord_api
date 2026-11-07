//! The Cloudflare API v4 response envelope, as the declared endpoints decode it.
//!
//! Every response is `{ "success", "errors", "messages", "result", "result_info" }`. These are
//! the declared outputs of the generated endpoints, so `prepare()?.send()` (Concord's own
//! handling, without the SDK's send policy) returns them as they arrived, failure included. The
//! generated `send()` goes through [`SendPolicy`](crate::SendPolicy), which turns a
//! [`Single::Failure`] or [`Paged::Failure`] into [`Error::Provider`](crate::Error::Provider) and
//! never into an empty result.
//!
//! `success` decides first. The envelope is decoded with `result` (and `result_info`) kept as
//! unparsed JSON, and they are parsed only when `success` is `true`: a refusal whose `result`
//! has an unexpected shape still reports Cloudflare's errors. `errors` and `messages` are
//! documented as always present; an absent one is read as empty.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

/// One entry of `errors` or `messages`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ResponseInfo {
    /// The numeric code (documented minimum 1000).
    pub code: u64,
    /// The provider's text.
    pub message: String,
    /// A link to documentation about the code.
    #[serde(default)]
    pub documentation_url: Option<String>,
    /// Where in the request the problem is.
    #[serde(default)]
    pub source: Option<ResponseInfoSource>,
}

/// `errors[].source`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ResponseInfoSource {
    /// A JSON pointer into the request.
    #[serde(default)]
    pub pointer: Option<String>,
}

/// The parts of an envelope that report a refusal: `errors` and `messages`. It is also how
/// the body of a non-2xx response is read.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct ErrorEnvelope {
    /// `errors`, in order.
    #[serde(default)]
    pub errors: Vec<ResponseInfo>,
    /// `messages`, in order.
    #[serde(default)]
    pub messages: Vec<ResponseInfo>,
}

impl ErrorEnvelope {
    /// The codes of `errors`, in order.
    pub fn codes(&self) -> Vec<u64> {
        self.errors.iter().map(|e| e.code).collect()
    }
}

/// `result_info` of a page-numbered listing (List Zones, List DNS Records). Every field is
/// documented as optional; the page walkers require `page` and `total_pages`.
///
/// `total_count` is documented as the total *without any search parameters*, so it is not a
/// count of a filtered listing and the walkers never use it to decide completeness.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub struct PageInfo {
    /// The current page.
    #[serde(default)]
    pub page: Option<u32>,
    /// Results per page.
    #[serde(default)]
    pub per_page: Option<u32>,
    /// `count` as Cloudflare sends it.
    #[serde(default)]
    pub count: Option<u64>,
    /// Total results without any search parameters.
    #[serde(default)]
    pub total_count: Option<u64>,
    /// Total pages of the result set.
    #[serde(default)]
    pub total_pages: Option<u32>,
}

/// The envelope of an endpoint that returns one object (Zone Details, DNS Record Details).
#[derive(Debug)]
pub enum Single<R> {
    /// `success: true`.
    Success {
        /// `result`, `None` when absent or `null`.
        result: Option<R>,
        /// `errors` sent beside `success: true` (the documented 200 examples carry one).
        errors: Vec<ResponseInfo>,
        /// `messages`.
        messages: Vec<ResponseInfo>,
    },
    /// `success: false`: Cloudflare refused the request.
    Failure(ErrorEnvelope),
}

/// The envelope of a page-numbered listing (List Zones, List DNS Records).
#[derive(Debug)]
pub enum Paged<R> {
    /// `success: true`.
    Success {
        /// `result`, `None` when absent or `null`.
        result: Option<Vec<R>>,
        /// `result_info`, `None` when absent or `null`.
        result_info: Option<PageInfo>,
        /// `errors` sent beside `success: true`.
        errors: Vec<ResponseInfo>,
        /// `messages`.
        messages: Vec<ResponseInfo>,
    },
    /// `success: false`: Cloudflare refused the request.
    Failure(ErrorEnvelope),
}

#[derive(Deserialize)]
struct Raw {
    success: bool,
    #[serde(default)]
    errors: Vec<ResponseInfo>,
    #[serde(default)]
    messages: Vec<ResponseInfo>,
    #[serde(default)]
    result: Option<Box<RawValue>>,
    #[serde(default)]
    result_info: Option<Box<RawValue>>,
}

fn parse<T: DeserializeOwned, E: serde::de::Error>(
    raw: Option<Box<RawValue>>,
) -> Result<Option<T>, E> {
    raw.map(|raw| serde_json::from_str(raw.get()).map_err(E::custom))
        .transpose()
}

impl<'de, R: DeserializeOwned> Deserialize<'de> for Single<R> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Raw::deserialize(deserializer)?;
        if !raw.success {
            return Ok(Self::Failure(ErrorEnvelope {
                errors: raw.errors,
                messages: raw.messages,
            }));
        }
        Ok(Self::Success {
            result: parse(raw.result)?,
            errors: raw.errors,
            messages: raw.messages,
        })
    }
}

impl<'de, R: DeserializeOwned> Deserialize<'de> for Paged<R> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Raw::deserialize(deserializer)?;
        if !raw.success {
            return Ok(Self::Failure(ErrorEnvelope {
                errors: raw.errors,
                messages: raw.messages,
            }));
        }
        Ok(Self::Success {
            result: parse(raw.result)?,
            result_info: parse(raw.result_info)?,
            errors: raw.errors,
            messages: raw.messages,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Zone;

    const REFUSAL: &str = include_str!("../fixtures/failure_unexpected_result.json");

    /// The control: an envelope derived in one serde pass loses the refusal when `result` has
    /// another shape than the endpoint's.
    #[test]
    fn a_derived_envelope_cannot_read_this_refusal() {
        #[derive(Deserialize)]
        #[allow(dead_code)]
        struct Derived {
            success: bool,
            errors: Vec<ResponseInfo>,
            result: Option<Vec<Zone>>,
            result_info: Option<PageInfo>,
        }
        assert!(serde_json::from_str::<Derived>(REFUSAL).is_err());
    }

    #[test]
    fn success_is_read_first() {
        let Paged::<Zone>::Failure(document) = serde_json::from_str(REFUSAL).unwrap() else {
            panic!("expected the failure");
        };
        assert_eq!(document.codes(), [1003]);
        let Single::<Zone>::Failure(document) = serde_json::from_str(REFUSAL).unwrap() else {
            panic!("expected the failure");
        };
        assert_eq!(document.codes(), [1003]);
    }

    #[test]
    fn a_success_with_a_malformed_result_is_an_error() {
        let body = r#"{"success": true, "errors": [], "messages": [], "result": {"name": 7}}"#;
        assert!(serde_json::from_str::<Single<Zone>>(body).is_err());
        let body = r#"{"success": true, "result": [], "result_info": {"page": "one"}}"#;
        assert!(serde_json::from_str::<Paged<Zone>>(body).is_err());
    }
}
