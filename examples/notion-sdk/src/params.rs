//! The deliberately small query vocabulary.

use serde::Serialize;
use std::fmt;

/// A Notion page size in the inclusive range 1..=100.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u8);

impl PageSize {
    /// Validates a requested size before any request is built.
    pub fn new(size: u8) -> Result<Self, PageSizeError> {
        if (1..=100).contains(&size) {
            Ok(Self(size))
        } else {
            Err(PageSizeError)
        }
    }

    /// The validated wire value.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for PageSize {
    fn default() -> Self {
        Self(100)
    }
}

/// A page size was outside Notion's supported range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSizeError;
impl fmt::Display for PageSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("page_size must be between 1 and 100")
    }
}
impl std::error::Error for PageSizeError {}

/// A select property's option name must equal this value.
#[derive(Clone, Debug, Serialize)]
pub struct SelectEquals {
    property: String,
    select: Equals,
}

#[derive(Clone, Debug, Serialize)]
struct Equals {
    equals: String,
}

impl SelectEquals {
    /// Filters by the property's name or ID and a select option's name.
    pub fn new(property: impl Into<String>, option: impl Into<String>) -> Self {
        Self {
            property: property.into(),
            select: Equals {
                equals: option.into(),
            },
        }
    }
}

/// Fixed query parameters reused unchanged on every cursor request.
#[derive(Clone, Debug, Default)]
pub struct Query {
    /// Optional select-equals condition; other filter families are outside this slice.
    pub filter: Option<SelectEquals>,
    /// Maximum number of results in one response.
    pub page_size: PageSize,
}

/// JSON body used by the declared query operation.
#[derive(Clone, Debug, Serialize)]
pub struct QueryBody {
    /// The supported filter, omitted when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<SelectEquals>,
    /// Opaque continuation, omitted on the first request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<String>,
    /// Validated maximum result count.
    pub page_size: u8,
}

impl Query {
    pub(crate) fn body(&self, cursor: Option<String>) -> QueryBody {
        QueryBody {
            filter: self.filter.clone(),
            start_cursor: cursor,
            page_size: self.page_size.get(),
        }
    }
}
