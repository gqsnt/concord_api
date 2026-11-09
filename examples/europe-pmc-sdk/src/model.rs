use crate::{Cursor, Error, PublicationId, RecordKey, Source};
use serde::Deserialize;
use std::fmt;

/// Key metadata shared by lite search, core search and citation results.
/// Unknown provider fields are intentionally tolerated; identity is mandatory.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(concord::Debug)]
pub struct Publication {
    /// External publication identifier.
    #[sensitive]
    pub id: PublicationId,
    /// Source to use together with the identifier.
    #[sensitive]
    pub source: Source,
    /// Publication title when supplied by the result type.
    #[sensitive]
    pub title: Option<String>,
    /// Provider-formatted author string.
    #[sensitive]
    pub author_string: Option<String>,
    /// Journal title, if present.
    #[sensitive]
    pub journal_title: Option<String>,
    /// Publication year as provider text.
    #[sensitive]
    pub pub_year: Option<String>,
    /// DOI, if present.
    #[sensitive]
    pub doi: Option<String>,
    /// Abstract from a core result, if available.
    #[serde(rename = "abstractText")]
    #[sensitive]
    pub abstract_text: Option<String>,
}

impl Publication {
    /// Identity for citations or a core metadata lookup.
    pub fn key(&self) -> RecordKey {
        RecordKey {
            source: self.source.clone(),
            id: self.id.clone(),
        }
    }
}

/// Nested result container used by both response formats.
#[derive(Debug, Deserialize)]
pub struct ResultList {
    /// Records in provider order; an empty list is valid.
    #[serde(default)]
    pub result: Vec<Publication>,
}

/// A typed JSON or XML search response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    /// Provider hit estimate, not used to manufacture cursors or completeness.
    pub hit_count: u64,
    /// Exact opaque next cursor; absence is the end of this chain.
    pub next_cursor_mark: Option<Cursor>,
    /// Search results.
    pub result_list: ResultList,
}

/// Nested citations response container.
#[derive(Debug, Deserialize)]
pub struct CitationList {
    /// Publications citing the selected publication.
    #[serde(default)]
    pub citation: Vec<Publication>,
}

/// One explicitly bounded citations page (this SDK always requests page 1).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitationsPage {
    /// Total number of citing publications reported by the provider.
    pub hit_count: u64,
    /// Citing publications on this page.
    pub citation_list: CitationList,
}

/// One entry of a publication's reference list. Unknown provider fields are tolerated.
///
/// Unlike [`Publication`], identity is optional: a reference the provider did not match to a
/// record has no source/identifier, and it is still part of the list and of the hit count.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reference {
    /// External identifier of the matched record, if any.
    pub id: Option<PublicationId>,
    /// Source of the matched record, if any.
    pub source: Option<Source>,
    /// Referenced title, if supplied.
    pub title: Option<String>,
    /// Provider-formatted author string.
    pub author_string: Option<String>,
    /// Journal abbreviation, if present.
    pub journal_abbreviation: Option<String>,
    /// Publication year as text; a JSON number is accepted and kept as its decimal text.
    #[serde(default, deserialize_with = "text_or_unsigned")]
    pub pub_year: Option<String>,
}

impl Reference {
    /// Identity for citations, references or a core lookup, when the reference was matched.
    pub fn key(&self) -> Option<RecordKey> {
        Some(RecordKey {
            source: self.source.clone()?,
            id: self.id.clone()?,
        })
    }
}

impl fmt::Debug for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reference")
            .field("matched", &self.key().is_some())
            .field("content", &"<redacted>")
            .finish()
    }
}

fn text_or_unsigned<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Year {
        Text(String),
        Number(u64),
    }
    Ok(Option::<Year>::deserialize(d)?.map(|year| match year {
        Year::Text(text) => text,
        Year::Number(number) => number.to_string(),
    }))
}

/// Nested reference-list container used by both response formats.
#[derive(Debug, Default, Deserialize)]
pub struct ReferenceList {
    /// References in provider order on this page; an empty list is valid.
    #[serde(default)]
    pub reference: Vec<Reference>,
}

/// One page of a publication's reference list (JSON or XML).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencesPage {
    /// Total number of references reported by the provider; it fixes the last page.
    pub hit_count: u64,
    /// References on this page. An absent list reads as empty; the traversal still requires
    /// the record count that the hit count and page number imply.
    #[serde(default)]
    pub reference_list: ReferenceList,
}

/// Success-envelope checks applied by the SDK's ordinary send policy.
pub trait Document {
    /// Refuses contradictory or excessive records after decoding.
    fn validate(&self, requested: usize) -> Result<(), Error>;
}

fn validate_count(count: usize, hits: u64, requested: usize) -> Result<(), Error> {
    if count > requested.min(100) || count as u64 > hits {
        Err(Error::InvalidPage(
            "record count exceeds requested size, SDK cap or hit count",
        ))
    } else {
        Ok(())
    }
}

impl Document for SearchPage {
    fn validate(&self, requested: usize) -> Result<(), Error> {
        validate_count(self.result_list.result.len(), self.hit_count, requested)
    }
}

impl Document for CitationsPage {
    fn validate(&self, requested: usize) -> Result<(), Error> {
        validate_count(self.citation_list.citation.len(), self.hit_count, requested)
    }
}

impl Document for ReferencesPage {
    fn validate(&self, requested: usize) -> Result<(), Error> {
        validate_count(
            self.reference_list.reference.len(),
            self.hit_count,
            requested,
        )
    }
}
