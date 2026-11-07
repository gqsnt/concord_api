use crate::{Cursor, Error, PublicationId, RecordKey, Source};
use serde::Deserialize;
use std::fmt;

/// Key metadata shared by lite search, core search and citation results.
/// Unknown provider fields are intentionally tolerated; identity is mandatory.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    /// External publication identifier.
    pub id: PublicationId,
    /// Source to use together with the identifier.
    pub source: Source,
    /// Publication title when supplied by the result type.
    pub title: Option<String>,
    /// Provider-formatted author string.
    pub author_string: Option<String>,
    /// Journal title, if present.
    pub journal_title: Option<String>,
    /// Publication year as provider text.
    pub pub_year: Option<String>,
    /// DOI, if present.
    pub doi: Option<String>,
    /// Abstract from a core result, if available.
    #[serde(rename = "abstractText")]
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

impl fmt::Debug for Publication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Publication")
            .field("content", &"<redacted>")
            .finish()
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
