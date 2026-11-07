//! A typed customer query: the documented subset of the select statement this slice needs, never
//! a free-form SQL string (the full query dialect is out of scope).

use std::fmt;
use std::num::NonZeroU32;

use crate::{EntityId, Error};

/// Which customers by `Active` state. QuickBooks returns active customers unless the statement
/// asks for both (`Active IN (true, false)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// `Active = true`.
    Active,
    /// `Active = false`.
    Inactive,
    /// `Active IN (true, false)`.
    Any,
}

/// A value compared with `DisplayName`: 1 to 500 characters (the field's maximum), without
/// control characters, `\` or `%`. An apostrophe is escaped with a backslash, as the query
/// syntax documents; `%` (the `LIKE` wildcard) and `\` have no documented escape, so they are
/// refused rather than guessed.
#[derive(Clone, PartialEq, Eq)]
struct Text(String);

impl Text {
    fn new(value: &str) -> Result<Self, Error> {
        let count = value.chars().count();
        let refused = value
            .chars()
            .any(|c| c.is_control() || c == '\\' || c == '%');
        if count == 0 || count > 500 || refused {
            return Err(Error::InvalidInput {
                what: "customer query text",
            });
        }
        Ok(Self(value.replace('\'', "\\'")))
    }
}

#[derive(Clone, PartialEq, Eq)]
enum NameFilter {
    Equals(Text),
    StartsWith(Text),
}

/// `SELECT * FROM Customer [WHERE ..] [ORDERBY DisplayName] STARTPOSITION n MAXRESULTS m`.
///
/// `Display` is the statement sent as the `query` parameter (marked sensitive in the
/// declaration: Concord never prints it). `Debug` withholds the compared text.
///
/// ```
/// use concord_sdk_quickbooks_online::{Activity, CustomerQuery};
/// let query = CustomerQuery::new()
///     .activity(Activity::Any)
///     .display_name_starts_with("Amy's")?
///     .order_by_display_name()
///     .max_results(50)?;
/// assert_eq!(
///     query.to_string(),
///     "SELECT * FROM Customer WHERE Active IN (true, false) AND DisplayName LIKE 'Amy\\'s%' \
///      ORDERBY DisplayName STARTPOSITION 1 MAXRESULTS 50"
/// );
/// # Ok::<(), concord_sdk_quickbooks_online::Error>(())
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct CustomerQuery {
    activity: Option<Activity>,
    id: Option<EntityId>,
    name: Option<NameFilter>,
    order_by_display_name: bool,
    start_position: NonZeroU32,
    max_results: u32,
}

impl Default for CustomerQuery {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomerQuery {
    /// The most entities one query response may hold.
    pub const MAX_RESULTS: u32 = 1000;
    /// The page size when none is chosen (QuickBooks' own default, sent explicitly).
    pub const DEFAULT_RESULTS: u32 = 100;

    /// Every customer QuickBooks returns by default, from position 1, 100 per page.
    pub fn new() -> Self {
        Self {
            activity: None,
            id: None,
            name: None,
            order_by_display_name: false,
            start_position: NonZeroU32::MIN,
            max_results: Self::DEFAULT_RESULTS,
        }
    }

    /// Filters on `Active`.
    #[must_use]
    pub fn activity(mut self, activity: Activity) -> Self {
        self.activity = Some(activity);
        self
    }

    /// `Id = '..'`.
    #[must_use]
    pub fn id(mut self, id: EntityId) -> Self {
        self.id = Some(id);
        self
    }

    /// `DisplayName = '..'` (comparisons are case-insensitive in QuickBooks).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for text this SDK refuses (see the type documentation).
    pub fn display_name(mut self, name: &str) -> Result<Self, Error> {
        self.name = Some(NameFilter::Equals(Text::new(name)?));
        Ok(self)
    }

    /// `DisplayName LIKE '..%'`.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for text this SDK refuses (see the type documentation).
    pub fn display_name_starts_with(mut self, prefix: &str) -> Result<Self, Error> {
        self.name = Some(NameFilter::StartsWith(Text::new(prefix)?));
        Ok(self)
    }

    /// `ORDERBY DisplayName` (a sortable field), for a stable order across pages.
    #[must_use]
    pub fn order_by_display_name(mut self) -> Self {
        self.order_by_display_name = true;
        self
    }

    /// `MAXRESULTS`: 1 to [`MAX_RESULTS`](Self::MAX_RESULTS).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] outside that range.
    pub fn max_results(mut self, count: u32) -> Result<Self, Error> {
        if count == 0 || count > Self::MAX_RESULTS {
            return Err(Error::InvalidInput {
                what: "MAXRESULTS (1 to 1000)",
            });
        }
        self.max_results = count;
        Ok(self)
    }

    /// `STARTPOSITION`, one-based.
    #[must_use]
    pub fn start_position(mut self, position: NonZeroU32) -> Self {
        self.start_position = position;
        self
    }

    /// The `STARTPOSITION` sent.
    pub fn position(&self) -> NonZeroU32 {
        self.start_position
    }

    /// The `MAXRESULTS` sent.
    pub fn page_size(&self) -> u32 {
        self.max_results
    }
}

impl fmt::Display for CustomerQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SELECT * FROM Customer")?;
        let mut clause = " WHERE ";
        if let Some(activity) = self.activity {
            f.write_str(clause)?;
            f.write_str(match activity {
                Activity::Active => "Active = true",
                Activity::Inactive => "Active = false",
                Activity::Any => "Active IN (true, false)",
            })?;
            clause = " AND ";
        }
        if let Some(id) = &self.id {
            write!(f, "{clause}Id = '{id}'")?;
            clause = " AND ";
        }
        match &self.name {
            Some(NameFilter::Equals(text)) => write!(f, "{clause}DisplayName = '{}'", text.0)?,
            Some(NameFilter::StartsWith(text)) => {
                write!(f, "{clause}DisplayName LIKE '{}%'", text.0)?;
            }
            None => {}
        }
        if self.order_by_display_name {
            f.write_str(" ORDERBY DisplayName")?;
        }
        write!(
            f,
            " STARTPOSITION {} MAXRESULTS {}",
            self.start_position, self.max_results
        )
    }
}

impl fmt::Debug for CustomerQuery {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CustomerQuery")
            .field("activity", &self.activity)
            .field("id", &self.id)
            .field(
                "display_name",
                &self.name.as_ref().map(|name| match name {
                    NameFilter::Equals(_) => "equals <withheld>",
                    NameFilter::StartsWith(_) => "starts with <withheld>",
                }),
            )
            .field("order_by_display_name", &self.order_by_display_name)
            .field("start_position", &self.start_position)
            .field("max_results", &self.max_results)
            .finish()
    }
}
