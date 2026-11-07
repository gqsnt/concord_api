use std::fmt;

use serde::{Deserialize, Serialize};

use crate::Error;

/// Valid Intercom contact page size. The SDK refuses values outside 1..=150.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct PageSize(u8);

impl PageSize {
    /// Validate a size without silently clamping it to the provider maximum.
    pub fn new(size: u16) -> Result<Self, Error> {
        if (1..=150).contains(&size) {
            Ok(Self(size as u8))
        } else {
            Err(Error::PageSize)
        }
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

/// Exact-email search input, including pagination in the JSON body.
/// Intercom validates the email; it is sent verbatim with the `=` operator.
#[derive(Clone, Serialize)]
pub struct EmailSearch {
    query: EmailQuery,
    pagination: SearchPagination,
}

#[derive(Clone, Serialize)]
struct EmailQuery {
    field: &'static str,
    operator: &'static str,
    value: String,
}

#[derive(Clone, Serialize)]
struct SearchPagination {
    per_page: PageSize,
    #[serde(skip_serializing_if = "Option::is_none")]
    starting_after: Option<String>,
}

impl EmailSearch {
    /// Match an email using provider equality. Search defaults to 50 results per page.
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            query: EmailQuery {
                field: "email",
                operator: "=",
                value: email.into(),
            },
            pagination: SearchPagination {
                per_page: PageSize(50),
                starting_after: None,
            },
        }
    }

    /// Select a validated page size without changing the email or cursor.
    pub fn with_per_page(mut self, size: PageSize) -> Self {
        self.pagination.per_page = size;
        self
    }

    /// Set or clear the opaque cursor for one `send()` call.
    /// A traversal's `pages(from, max_pages)` replaces this cursor with `from`.
    pub fn with_starting_after(mut self, cursor: Option<String>) -> Self {
        self.pagination.starting_after = cursor;
        self
    }
}

/// Contact role when creating a contact.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// An identified user.
    User,
    /// A lead.
    Lead,
}

/// Email-based create input. Other create modes and custom attributes are outside this slice.
#[derive(Clone, Serialize)]
pub struct NewContact {
    /// Provider-validated email address.
    pub email: String,
    /// Omitted unless selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Omitted unless selected; Intercom chooses its default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
}

impl NewContact {
    /// Create by email with no optional fields. Intercom validates the address.
    pub fn with_email(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            name: None,
            role: None,
        }
    }
}

#[derive(Deserialize)]
enum ContactType {
    #[serde(rename = "contact")]
    Contact,
}
#[derive(Deserialize)]
enum ListType {
    #[serde(rename = "list")]
    List,
}

/// Selected contact fields; unknown provider fields are ignored for forward compatibility.
#[derive(Deserialize)]
pub struct Contact {
    #[serde(rename = "type")]
    _kind: ContactType,
    /// The Intercom identifier; this is the identifier accepted by `contacts().get`.
    pub id: String,
    /// Provider role, retained as text to allow future roles.
    pub role: String,
    /// Client-provided external identifier, if present.
    pub external_id: Option<String>,
    /// Email, if present. Provider HTML escaping is preserved.
    pub email: Option<String>,
    /// Name, if present. Provider HTML escaping is preserved.
    pub name: Option<String>,
}

/// Provider list envelope. Missing `pages` is an error, while explicit null is accepted.
#[derive(Deserialize)]
pub struct ContactList {
    #[serde(rename = "type")]
    _kind: ListType,
    /// Contacts in this page.
    pub data: Vec<Contact>,
    /// Provider count; never used to invent a continuation.
    pub total_count: u64,
    /// Cursor metadata; explicit null is a terminal page.
    #[serde(deserialize_with = "Option::deserialize")]
    pub pages: Option<CursorPages>,
}

/// Only `next` determines pagination; numeric page counts are deliberately ignored.
#[derive(Deserialize)]
pub struct CursorPages {
    /// Absent or null on the last page.
    pub next: Option<CursorNext>,
}

/// The next cursor, used verbatim in a list query or search JSON body, never as a URL.
#[derive(Deserialize)]
pub struct CursorNext {
    /// Opaque provider cursor. The traversal refuses an empty or immediately repeated cursor.
    pub starting_after: String,
}

macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$(
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($ty), " { <redacted> }"))
            }
        }
    )+};
}
redacted_debug!(
    Contact,
    ContactList,
    CursorPages,
    CursorNext,
    NewContact,
    EmailSearch
);
