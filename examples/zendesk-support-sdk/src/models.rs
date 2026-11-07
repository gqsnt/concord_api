//! Provider models of the ticket slice and the validated inputs the declaration places.

use std::fmt;

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::Error;

/// A string-valued provider enumeration that keeps values this SDK does not know.
macro_rules! wire_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident => $wire:literal,)+ }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
            /// A value this SDK does not know, kept verbatim.
            Other(String),
        }

        impl $name {
            /// The wire spelling.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(value) => value,
                }
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value),
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                String::deserialize(d).map(Self::from)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }
    };
}

wire_enum!(
    /// A ticket's `status` (documented values; a custom status reports its category here).
    Status {
        /// `new`
        New => "new",
        /// `open`
        Open => "open",
        /// `pending`
        Pending => "pending",
        /// `hold`
        Hold => "hold",
        /// `solved`
        Solved => "solved",
        /// `closed`
        Closed => "closed",
    }
);

wire_enum!(
    /// A ticket's `priority`.
    Priority {
        /// `urgent`
        Urgent => "urgent",
        /// `high`
        High => "high",
        /// `normal`
        Normal => "normal",
        /// `low`
        Low => "low",
    }
);

wire_enum!(
    /// A ticket's `type`.
    TicketType {
        /// `problem`
        Problem => "problem",
        /// `incident`
        Incident => "incident",
        /// `question`
        Question => "question",
        /// `task`
        Task => "task",
    }
);

/// One ticket, as the Tickets API returns it. Fields outside this slice are ignored; the struct
/// is `#[non_exhaustive]` so fields can be added without breaking callers.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct Ticket {
    /// Automatically assigned identifier.
    pub id: u64,
    /// The API URL of this ticket.
    pub url: String,
    /// An identifier the application chose (`external_id`), usable to find the ticket again.
    pub external_id: Option<String>,
    /// `problem`, `incident`, `question` or `task`.
    #[serde(rename = "type")]
    pub ticket_type: Option<TicketType>,
    /// The subject.
    pub subject: Option<String>,
    /// The first comment's text (read-only).
    pub description: Option<String>,
    /// `urgent`, `high`, `normal` or `low`.
    pub priority: Option<Priority>,
    /// The ticket's state.
    pub status: Status,
    /// The requester's user id.
    pub requester_id: Option<u64>,
    /// The submitter's user id.
    pub submitter_id: Option<u64>,
    /// The assignee's user id.
    pub assignee_id: Option<u64>,
    /// The organization id.
    pub organization_id: Option<u64>,
    /// The group id.
    pub group_id: Option<u64>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Creation time, ISO 8601, as sent.
    pub created_at: String,
    /// Last update time, ISO 8601, as sent.
    pub updated_at: String,
}

/// `{"ticket": {..}}`: the Show Ticket response.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct TicketResponse {
    /// The ticket.
    pub ticket: Ticket,
}

/// A page of List Tickets under cursor pagination.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[non_exhaustive]
pub struct TicketPage {
    /// The tickets of this page (at most `page[size]`).
    pub tickets: Vec<Ticket>,
    /// Cursor state; `has_more` decides whether another page exists.
    pub meta: CursorMeta,
    /// The provider's page links.
    #[serde(default)]
    pub links: CursorLinks,
}

/// The `meta` object of a cursor page.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct CursorMeta {
    /// Whether another page follows. The traversal ends exactly when this is `false`.
    pub has_more: bool,
    /// The cursor of the next page (`page[after]`).
    pub after_cursor: Option<String>,
    /// The cursor of the previous page (`page[before]`).
    pub before_cursor: Option<String>,
}

/// The `links` object of a cursor page.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct CursorLinks {
    /// URL of the next page.
    pub next: Option<String>,
    /// URL of the previous page.
    pub prev: Option<String>,
}

/// What `x-idempotency-lookup` said about a create sent with an `Idempotency-Key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdempotencyLookup {
    /// `miss`: this request created the ticket.
    Miss,
    /// `hit`: the key was already used with the same body; the original response was replayed
    /// and no second ticket was created.
    Hit,
    /// The header was absent.
    Absent,
    /// The header was repeated, not text, or a value the documentation does not define. The
    /// create still succeeded; only the lookup is unknown.
    Unrecognized,
}

/// The Create Ticket response, with the idempotency lookup read from its head.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CreatedTicket {
    /// The ticket as created (or as created by the original request, on a `hit`).
    pub ticket: Ticket,
    /// `x-idempotency-lookup`.
    pub idempotency: IdempotencyLookup,
}

/// A new ticket. The only required property is the first comment's text.
///
/// Built with [`NewTicket::new`] and the setters; it serializes as Zendesk's envelope
/// `{"ticket": {"comment": {"body": ..}, ..}}`, leaving unset properties out.
#[derive(Clone, Debug, PartialEq)]
pub struct NewTicket {
    comment: String,
    subject: Option<String>,
    priority: Option<Priority>,
    ticket_type: Option<TicketType>,
    external_id: Option<String>,
    tags: Vec<String>,
}

impl NewTicket {
    /// A ticket whose first comment is `comment` (plain text, `comment.body`).
    pub fn new(comment: impl Into<String>) -> Self {
        Self {
            comment: comment.into(),
            subject: None,
            priority: None,
            ticket_type: None,
            external_id: None,
            tags: Vec::new(),
        }
    }

    /// Sets `subject`.
    #[must_use]
    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Sets `priority`.
    #[must_use]
    pub fn priority(mut self, priority: Priority) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Sets `type`.
    #[must_use]
    pub fn ticket_type(mut self, ticket_type: TicketType) -> Self {
        self.ticket_type = Some(ticket_type);
        self
    }

    /// Sets `external_id`: the application's own identifier, by which
    /// [`list`](crate::ZendeskTickets::list)`().set_external_id(..)` finds the ticket again.
    #[must_use]
    pub fn external_id(mut self, external_id: impl Into<String>) -> Self {
        self.external_id = Some(external_id.into());
        self
    }

    /// Sets `tags`.
    #[must_use]
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }
}

impl Serialize for NewTicket {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Comment<'a> {
            body: &'a str,
        }
        #[derive(Serialize)]
        struct Inner<'a> {
            comment: Comment<'a>,
            #[serde(skip_serializing_if = "Option::is_none")]
            subject: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            priority: Option<&'a Priority>,
            #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
            ticket_type: Option<&'a TicketType>,
            #[serde(skip_serializing_if = "Option::is_none")]
            external_id: Option<&'a str>,
            #[serde(skip_serializing_if = "<[String]>::is_empty")]
            tags: &'a [String],
        }
        let mut map = s.serialize_map(Some(1))?;
        map.serialize_entry(
            "ticket",
            &Inner {
                comment: Comment {
                    body: &self.comment,
                },
                subject: self.subject.as_deref(),
                priority: self.priority.as_ref(),
                ticket_type: self.ticket_type.as_ref(),
                external_id: self.external_id.as_deref(),
                tags: &self.tags,
            },
        )?;
        map.end()
    }
}

/// The value of the `Idempotency-Key` header of a ticket create.
///
/// Zendesk replays the original response for a repeated key and body within two hours, and
/// refuses the key with another body (`400 IdempotentRequestError`). Derive it from an identifier
/// the application stores *before* sending (its own record id), so that after an uncertain
/// create it can replay exactly the same request, even from another process.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// A key of visible ASCII without spaces (this SDK's rule, so the header value is sent and
    /// compared exactly as written).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an empty key or one with other characters; nothing is sent.
    pub fn new(key: impl Into<String>) -> Result<Self, Error> {
        let key = key.into();
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_graphic()) {
            return Err(Error::InvalidInput {
                what: "idempotency key",
            });
        }
        Ok(Self(key))
    }

    /// The key as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `page[size]` of a cursor-paginated list: 1 to 100 records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageSize(u8);

impl PageSize {
    /// The documented maximum, 100: the default of every listing.
    pub const MAX: Self = Self(100);

    /// A page size in `1..=100`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside that range; nothing is sent.
    pub fn new(size: u8) -> Result<Self, Error> {
        if (1..=Self::MAX.0).contains(&size) {
            Ok(Self(size))
        } else {
            Err(Error::InvalidInput { what: "page size" })
        }
    }

    /// The validated value.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for PageSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A field cursor pagination can sort List Tickets by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortField {
    /// `updated_at`
    UpdatedAt,
    /// `id`
    Id,
    /// `status`
    Status,
}

/// The `sort` parameter of a cursor-paginated List Tickets: a field, ascending or descending
/// (`-field`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TicketSort {
    field: SortField,
    descending: bool,
}

impl TicketSort {
    /// Ascending by `field`.
    pub const fn ascending(field: SortField) -> Self {
        Self {
            field,
            descending: false,
        }
    }

    /// Descending by `field`.
    pub const fn descending(field: SortField) -> Self {
        Self {
            field,
            descending: true,
        }
    }
}

impl fmt::Display for TicketSort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.descending {
            f.write_str("-")?;
        }
        f.write_str(match self.field {
            SortField::UpdatedAt => "updated_at",
            SortField::Id => "id",
            SortField::Status => "status",
        })
    }
}
