use crate::Error;
use serde::{Serialize, Serializer};

/// Longest documented `ignoreWindow` / `ignoreUserWindow`: 7 days, in minutes.
pub const MAX_IGNORE_WINDOW_MINUTES: u32 = 7 * 24 * 60;

/// One issue status change for `PUT .../issues/{issue}/` ("Update an Issue").
///
/// The JSON body is `status` plus only the details of this change: no other issue attribute
/// (assignment, bookmark, inbox, merge, discard, ...) and no `null` is ever sent, so Sentry
/// modifies only what the caller means. Statuses form a closed set; a status outside it cannot
/// be written. Documented values not offered here (`muted`, release or commit resolution
/// details, `substatus`) are not sent at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StatusUpdate {
    /// `{"status":"unresolved"}`.
    Unresolved,
    /// `{"status":"resolved"}`.
    Resolved,
    /// `{"status":"resolvedInNextRelease"}`.
    ResolvedInNextRelease,
    /// `{"status":"ignored"}` plus the `ignore*` fields of its condition.
    Ignored(Ignore),
}

/// When an ignored issue resurfaces. Build it with the checked constructors: a zero count,
/// duration or window, or a window over [`MAX_IGNORE_WINDOW_MINUTES`], is refused with
/// [`Error::InvalidUpdate`] before any request exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ignore(Condition);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Condition {
    Forever,
    Minutes(u32),
    Seen { times: u32, window: Option<u32> },
    Users { users: u32, window: Option<u32> },
}

impl Ignore {
    /// No condition: ignored until another status change.
    pub const FOREVER: Self = Self(Condition::Forever);

    /// `ignoreDuration`: ignored for this many minutes.
    pub fn for_minutes(minutes: u32) -> Result<Self, Error> {
        Ok(Self(Condition::Minutes(positive(
            minutes,
            "zero ignore duration",
        )?)))
    }

    /// `ignoreCount` (with `ignoreWindow`): until seen this many more times, counted over the
    /// whole future or within a window of minutes.
    pub fn until_seen(times: u32, window_minutes: Option<u32>) -> Result<Self, Error> {
        Ok(Self(Condition::Seen {
            times: positive(times, "zero ignore count")?,
            window: window(window_minutes)?,
        }))
    }

    /// `ignoreUserCount` (with `ignoreUserWindow`): until it affects this many more users,
    /// counted over the whole future or within a window of minutes.
    pub fn until_users(users: u32, window_minutes: Option<u32>) -> Result<Self, Error> {
        Ok(Self(Condition::Users {
            users: positive(users, "zero ignore user count")?,
            window: window(window_minutes)?,
        }))
    }
}

fn positive(value: u32, reason: &'static str) -> Result<u32, Error> {
    if value == 0 {
        Err(Error::InvalidUpdate(reason))
    } else {
        Ok(value)
    }
}

fn window(minutes: Option<u32>) -> Result<Option<u32>, Error> {
    minutes
        .map(|minutes| match minutes {
            0 => Err(Error::InvalidUpdate("zero ignore window")),
            1..=MAX_IGNORE_WINDOW_MINUTES => Ok(minutes),
            _ => Err(Error::InvalidUpdate("ignore window exceeds 7 days")),
        })
        .transpose()
}

/// The wire body: absent fields are omitted, never `null`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_duration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_window: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_user_count: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ignore_user_window: Option<u32>,
}

impl Serialize for StatusUpdate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let status = |status| Wire {
            status,
            ignore_duration: None,
            ignore_count: None,
            ignore_window: None,
            ignore_user_count: None,
            ignore_user_window: None,
        };
        let wire = match *self {
            Self::Unresolved => status("unresolved"),
            Self::Resolved => status("resolved"),
            Self::ResolvedInNextRelease => status("resolvedInNextRelease"),
            Self::Ignored(Ignore(condition)) => {
                let base = status("ignored");
                match condition {
                    Condition::Forever => base,
                    Condition::Minutes(minutes) => Wire {
                        ignore_duration: Some(minutes),
                        ..base
                    },
                    Condition::Seen { times, window } => Wire {
                        ignore_count: Some(times),
                        ignore_window: window,
                        ..base
                    },
                    Condition::Users { users, window } => Wire {
                        ignore_user_count: Some(users),
                        ignore_user_window: window,
                        ..base
                    },
                }
            }
        };
        wire.serialize(serializer)
    }
}
