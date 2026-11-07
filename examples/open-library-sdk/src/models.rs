use std::{fmt, str::FromStr};

use serde::Deserialize;
use serde_json::Value;

use crate::{Error, RecordKind};

macro_rules! identifier {
    ($name:ident, $kind:ident, $prefix:literal, $suffix:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Accepts a bare OLID or its canonical resource key, without a slug or suffix.
            ///
            /// This SDK validates `OL` + ASCII digits + the resource letter. It never follows
            /// URLs found in provider documents. Invalid input is not included in the error.
            pub fn parse(value: &str) -> Result<Self, Error> {
                let bare = value.strip_prefix($prefix).unwrap_or(value);
                let digits = bare
                    .strip_prefix("OL")
                    .and_then(|v| v.strip_suffix($suffix));
                if !digits.is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())) {
                    return Err(Error::InvalidId(RecordKind::$kind));
                }
                Ok(Self(bare.to_owned()))
            }

            /// The bare identifier used in the path, for example `OL27448W`.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::parse(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

identifier!(
    WorkId,
    Work,
    "/works/",
    "W",
    "A validated work identifier, stored without `/works/`."
);
identifier!(
    AuthorId,
    Author,
    "/authors/",
    "A",
    "A validated author identifier, stored without `/authors/`."
);

/// Sort values listed on the [Search API page](https://openlibrary.org/dev/docs/api/search).
///
/// Omitting the search's sort option uses the provider's relevance order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WorkSort {
    /// Newest first (`new`).
    New,
    /// Oldest first (`old`).
    Old,
    /// Random order (`random`).
    Random,
    /// Lexicographic key order (`key`), not numeric OLID order.
    Key,
}

impl fmt::Display for WorkSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::New => "new",
            Self::Old => "old",
            Self::Random => "random",
            Self::Key => "key",
        })
    }
}

/// A search page. `docs: []` is an ordinary successful empty page.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct WorkSearch {
    /// Search hits. The envelope must contain an array, even when empty.
    pub docs: Vec<WorkHit>,
    /// A reported count, accepting both spellings shown in the search documentation.
    #[serde(rename = "numFound", alias = "num_found")]
    pub num_found: Option<u64>,
    /// Reported start position; not used to infer a continuation.
    pub start: Option<u64>,
    /// Whether the provider says its count is exact, when present.
    #[serde(rename = "numFoundExact")]
    pub num_found_exact: Option<bool>,
}

/// Selected common work-search fields. Unknown fields are ignored; metadata may be absent/null.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct WorkHit {
    /// Bare ID or canonical work key; deliberately retained as provider text.
    pub key: Option<String>,
    /// Display title, when supplied.
    pub title: Option<String>,
    /// Author names, when supplied.
    pub author_name: Option<Vec<String>>,
    /// First publication year, when supplied.
    pub first_publish_year: Option<i64>,
}

impl WorkHit {
    /// Validates a selected key for retrieval; missing or invalid keys return an input error.
    pub fn id(&self) -> Result<WorkId, Error> {
        WorkId::parse(self.key.as_deref().unwrap_or_default())
    }
}

/// An author-search page. An absent `docs` array is a decoding error, not an empty search.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct AuthorSearch {
    /// Author hits, possibly empty.
    pub docs: Vec<AuthorHit>,
    /// The reported count, when present.
    #[serde(rename = "numFound", alias = "num_found")]
    pub num_found: Option<u64>,
    /// Reported start position, when present.
    pub start: Option<u64>,
    /// Whether the reported count is exact, when present.
    #[serde(rename = "numFoundExact")]
    pub num_found_exact: Option<bool>,
}

/// Selected common author-search fields, with no promise of their presence.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct AuthorHit {
    /// Bare author ID or canonical author key.
    pub key: Option<String>,
    /// Display name, when supplied.
    pub name: Option<String>,
    /// The provider's top-work title, when supplied.
    pub top_work: Option<String>,
    /// The provider's work count, when supplied.
    pub work_count: Option<u64>,
}

impl AuthorHit {
    /// Validates a selected key for the author's works request.
    pub fn id(&self) -> Result<AuthorId, Error> {
        AuthorId::parse(self.key.as_deref().unwrap_or_default())
    }
}

/// A deliberately small projection of a work record. Unmodelled fields are ignored.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct Work {
    /// Provider key, when present.
    pub key: Option<String>,
    /// Display title, when present.
    pub title: Option<String>,
    /// Original description JSON; use [`Self::description_text`] for either text shape.
    pub description: Option<Value>,
    /// Subject labels, when present.
    pub subjects: Option<Vec<String>>,
}

impl Work {
    /// Borrows the description as text, whether it is a string or a `/type/text` object.
    ///
    /// Missing/null descriptions yield `None`; an empty string in either shape yields
    /// `Some("")`. Text is returned unchanged, including whitespace and markup.
    /// Unrecognized shapes yield `None` and remain available in [`Self::description`].
    ///
    /// The [data dumps documentation](https://openlibrary.org/developers/dumps)
    /// links the work schema and its `text_block` definition (`type` plus string `value`).
    pub fn description_text(&self) -> Option<&str> {
        match self.description.as_ref()? {
            Value::String(text) => Some(text),
            Value::Object(object)
                if object.get("type").and_then(Value::as_str) == Some("/type/text") =>
            {
                object.get("value").and_then(Value::as_str)
            }
            _ => None,
        }
    }
}

/// One batch of an author's works. The SDK does not claim this is the entire collection.
#[derive(Debug, Deserialize)]
#[non_exhaustive]
pub struct AuthorWorks {
    /// Work entries. Missing `entries` is a decoding error; an empty array is valid.
    pub entries: Vec<Work>,
    /// Reported collection size, when present; never treated as a stable snapshot guarantee.
    pub size: Option<u64>,
}
