use crate::Error;
use serde::Deserialize;
use std::{fmt, num::NonZeroU32};

macro_rules! text_value {
    ($name:ident, $doc:literal, $validate:expr, $message:literal) => {
        #[doc = $doc]
        #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
        #[serde(try_from = "String")]
        #[derive(concord::Debug, serde::Serialize)]
        pub struct $name(#[sensitive] String);

        impl $name {
            /// Validates provider text without changing or pre-encoding it.
            pub fn new(value: impl Into<String>) -> Result<Self, Error> {
                let value = value.into();
                if !($validate)(&value) {
                    return Err(Error::InvalidInput($message));
                }
                Ok(Self(value))
            }

            /// Unredacted provider text; use deliberately rather than logging it.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = Error;
            fn try_from(value: String) -> Result<Self, Error> {
                Self::new(value)
            }
        }

        serde_plain::derive_display_from_serialize!($name);
    };
}

fn bounded_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

text_value!(
    Query,
    "A nonblank search expression, at most 2048 UTF-8 bytes. Display is the wire value.",
    |v: &str| bounded_text(v, 2048),
    "invalid search query"
);
text_value!(
    Cursor,
    "An opaque cursor mark, at most 4096 bytes. Display is the wire value.",
    |v: &str| bounded_text(v, 4096),
    "invalid cursor mark"
);
text_value!(
    Source,
    "An uppercase provider source code (for example MED or PMC). Display is the wire value.",
    |v: &str| !v.is_empty()
        && v.len() <= 16
        && v.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
    "invalid publication source"
);
text_value!(
    PublicationId,
    "A publication identifier restricted to query-safe ASCII. Display is the wire value.",
    |v: &str| !v.is_empty()
        && v.len() <= 256
        && v != "."
        && v != ".."
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
    "invalid publication identifier"
);

impl Cursor {
    /// The provider's first-page marker.
    pub fn start() -> Self {
        Self("*".into())
    }
}

/// A selected publication's identity; both fields are required by the citations module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordKey {
    /// Provider source code.
    pub source: Source,
    /// External publication identifier within that source.
    pub id: PublicationId,
}

impl RecordKey {
    /// Validates a source and external identifier.
    pub fn new(source: impl Into<String>, id: impl Into<String>) -> Result<Self, Error> {
        Ok(Self {
            source: Source::new(source)?,
            id: PublicationId::new(id)?,
        })
    }

    pub(crate) fn query(&self) -> Query {
        // The two validated alphabets cannot introduce query operators or quoted strings.
        Query(format!("EXT_ID:{} AND SRC:{}", self.id, self.source))
    }
}

/// SDK page size: 1..=100 records, below the provider's documented maximum of 1000.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u16);

impl PageSize {
    /// Refuses zero or more than the SDK's 100-record cap.
    pub fn new(value: u16) -> Result<Self, Error> {
        if !(1..=100).contains(&value) {
            return Err(Error::InvalidInput("page size must be in 1..=100"));
        }
        Ok(Self(value))
    }

    /// Number of records requested.
    pub fn get(self) -> usize {
        usize::from(self.0)
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self(25)
    }
}

impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A one-based reference-list page number; the resume key of a references traversal.
/// It addresses `(n - 1) * page size` records, so it belongs to one publication and page size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PageNumber(NonZeroU32);

impl PageNumber {
    /// The first page.
    pub const FIRST: Self = Self(NonZeroU32::MIN);

    /// Refuses page zero; page numbers are one-based.
    pub fn new(value: u32) -> Result<Self, Error> {
        NonZeroU32::new(value)
            .map(Self)
            .ok_or(Error::InvalidInput("page number must be at least 1"))
    }

    /// The one-based page number sent as `page`.
    pub fn get(self) -> u32 {
        self.0.get()
    }

    /// Records before this page at `size`: the zero-based offset of its first record.
    pub(crate) fn offset(self, size: PageSize) -> u64 {
        u64::from(self.get() - 1) * size.get() as u64
    }

    pub(crate) fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

impl fmt::Display for PageNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
