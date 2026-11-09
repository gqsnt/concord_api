use crate::{Error, Field};
use std::fmt;

/// The one rule for text sent inside a double-quoted phrase (manual §5.1): 1–512 UTF-8
/// bytes, not blank, and no quote, backslash or control character that could end it.
pub(crate) fn valid_phrase(phrase: &str) -> bool {
    !phrase.trim().is_empty()
        && phrase.len() <= 512
        && !phrase
            .chars()
            .any(|c| c.is_control() || matches!(c, '"' | '\\'))
}

/// A literal term; `Display` supplies the provider's `all:` phrase syntax.
#[derive(Clone, concord::Debug)]
pub struct SearchTerm(#[sensitive] String);
impl SearchTerm {
    /// Accept 1–512 UTF-8 bytes, without quotes, backslashes or controls.
    pub fn new(term: impl Into<String>) -> Result<Self, Error> {
        let term = term.into();
        if !valid_phrase(&term) {
            return Err(Error::Input("invalid search term"));
        }
        Ok(Self(term))
    }
}
impl fmt::Display for SearchTerm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:\"{}\"", Field::All.prefix(), self.0)
    }
}

/// A delimiter-safe list of 1–50 bare arXiv identifiers. The service validates ID grammar.
#[derive(Clone, concord::Debug)]
pub struct IdList(#[sensitive] String);
impl IdList {
    /// Accept modern or legacy IDs (and version suffixes), without URLs or separators.
    pub fn new(ids: &[&str]) -> Result<Self, Error> {
        if ids.is_empty()
            || ids.len() > 50
            || ids.iter().any(|id| {
                id.is_empty()
                    || id.len() > 64
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-'))
            })
        {
            return Err(Error::Input("invalid identifier list"));
        }
        Ok(Self(ids.join(",")))
    }
}
impl fmt::Display for IdList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A deliberately small page of 1–100 entries (provider slice ceiling: 2,000).
#[derive(Clone, Copy, Debug)]
pub struct PageSize(u16);
impl PageSize {
    /// Refuse zero or more than 100 results per call.
    pub fn new(size: u16) -> Result<Self, Error> {
        if !(1..=100).contains(&size) {
            return Err(Error::Input("page size must be 1–100"));
        }
        Ok(Self(size))
    }
}
impl Default for PageSize {
    fn default() -> Self {
        Self(10)
    }
}
impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
