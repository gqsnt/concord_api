use crate::{Error, input::valid_phrase};
use std::fmt;

/// A searchable field: the field prefixes of the arXiv API manual, §5.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Field {
    /// `ti`: title.
    Title,
    /// `au`: author.
    Author,
    /// `abs`: abstract.
    Abstract,
    /// `co`: comment.
    Comment,
    /// `jr`: journal reference.
    JournalRef,
    /// `cat`: subject category, such as `hep-th` or `cs.AI`.
    Category,
    /// `rn`: report number.
    ReportNumber,
    /// `id`: identifier. The manual recommends `fetch` (`id_list`) to handle versions.
    Id,
    /// `all`: every field above at once.
    All,
}
impl Field {
    /// The documented prefix, written before the colon.
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Title => "ti",
            Self::Author => "au",
            Self::Abstract => "abs",
            Self::Comment => "co",
            Self::JournalRef => "jr",
            Self::Category => "cat",
            Self::ReportNumber => "rn",
            Self::Id => "id",
            Self::All => "all",
        }
    }
}

/// A `search_query` that is syntactically valid by construction.
///
/// Each term is one field prefix and one double-quoted phrase; terms combine with the
/// documented Boolean operators `AND`, `OR` and `ANDNOT`. A combined operand is always
/// parenthesized, so the expression's grouping is the order in which it was built.
/// Operator words, colons and parentheses inside a phrase are phrase text.
#[derive(Clone)]
pub struct Query {
    wire: String,
    compound: bool,
}
impl Query {
    /// `prefix:"phrase"`. The phrase follows `SearchTerm`'s rule: 1–512 UTF-8 bytes, not
    /// blank, without double quotes, backslashes or control characters.
    pub fn term(field: Field, phrase: impl Into<String>) -> Result<Self, Error> {
        let phrase = phrase.into();
        if !valid_phrase(&phrase) {
            return Err(Error::Input("invalid query term"));
        }
        Ok(Self {
            wire: format!("{}:\"{phrase}\"", field.prefix()),
            compound: false,
        })
    }
    /// `self AND other`: both match.
    #[must_use]
    pub fn and(self, other: Self) -> Self {
        self.join("AND", other)
    }
    /// `self OR other`: either matches.
    #[must_use]
    pub fn or(self, other: Self) -> Self {
        self.join("OR", other)
    }
    /// `self ANDNOT other`: `self` matches and `other` does not.
    #[must_use]
    pub fn and_not(self, other: Self) -> Self {
        self.join("ANDNOT", other)
    }
    fn join(self, operator: &str, other: Self) -> Self {
        let mut wire = String::with_capacity(self.wire.len() + other.wire.len() + 12);
        for (i, operand) in [self, other].into_iter().enumerate() {
            if i == 1 {
                wire.push(' ');
                wire.push_str(operator);
                wire.push(' ');
            }
            if operand.compound {
                wire.push('(');
                wire.push_str(&operand.wire);
                wire.push(')');
            } else {
                wire.push_str(&operand.wire);
            }
        }
        Self {
            wire,
            compound: true,
        }
    }
}
/// The provider syntax; the transport form-encodes it (`%22`, `%28`, `%29`, `+`).
impl fmt::Display for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.wire)
    }
}
impl fmt::Debug for Query {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Query(<redacted>)")
    }
}

/// `sortBy`: the manual's three result orderings, §3.1.1.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortBy {
    /// `relevance`.
    Relevance,
    /// `lastUpdatedDate`.
    LastUpdatedDate,
    /// `submittedDate`.
    SubmittedDate,
}
impl fmt::Display for SortBy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Relevance => "relevance",
            Self::LastUpdatedDate => "lastUpdatedDate",
            Self::SubmittedDate => "submittedDate",
        })
    }
}

/// `sortOrder`, §3.1.1.3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortOrder {
    /// `ascending`.
    Ascending,
    /// `descending`.
    Descending,
}
impl fmt::Display for SortOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        })
    }
}
