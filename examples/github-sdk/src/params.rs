//! Typed query, header and path values, each formatted exactly as GitHub documents it.
//!
//! Values that GitHub constrains are validated when they are constructed, so a refused value
//! never becomes a request: [`PerPage`] (1–100; GitHub silently clamps larger values, which would
//! hide a caller's mistake), [`Timestamp`] (`YYYY-MM-DDTHH:MM:SSZ`, the ISO 8601 form the
//! `since` parameters document), [`Labels`] (comma-separated names; a name containing a comma
//! cannot be expressed) and [`ApiVersion`] (a `YYYY-MM-DD` date).

use std::fmt;

use serde::{Deserialize, Serialize};

/// A value GitHub would not accept (or would silently reinterpret). Nothing was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamError {
    /// `per_page` must be 1 through 100.
    PerPage,
    /// Not of the form `YYYY-MM-DDTHH:MM:SSZ` with valid field ranges.
    Timestamp,
    /// A label list must be non-empty and no name may be empty or contain a comma.
    Labels,
    /// An API version must be a `YYYY-MM-DD` date.
    ApiVersion,
}

impl fmt::Display for ParamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::PerPage => "per_page must be 1 through 100",
            Self::Timestamp => "timestamp is not YYYY-MM-DDTHH:MM:SSZ",
            Self::Labels => "labels must be non-empty names without commas",
            Self::ApiVersion => "API version is not a YYYY-MM-DD date",
        })
    }
}

impl std::error::Error for ParamError {}

macro_rules! wire_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $wire:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant),+
        }

        impl $name {
            /// The wire text.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

wire_enum!(
    /// `state` of the issue and pull-request listings (default `open` at GitHub).
    StateFilter {
        /// `open`
        Open = "open",
        /// `closed`
        Closed = "closed",
        /// `all`
        All = "all",
    }
);

wire_enum!(
    /// `sort` of `GET .../issues` (default `created`).
    IssueSort {
        /// `created`
        Created = "created",
        /// `updated`
        Updated = "updated",
        /// `comments`
        Comments = "comments",
    }
);

wire_enum!(
    /// `sort` of `GET .../pulls` (default `created`).
    PullSort {
        /// `created`
        Created = "created",
        /// `updated`
        Updated = "updated",
        /// `popularity` (comment count)
        Popularity = "popularity",
        /// `long-running` (age, filtering out pull requests updated within the last month)
        LongRunning = "long-running",
    }
);

wire_enum!(
    /// `sort` of `GET .../issues/comments` (default `created`).
    CommentSort {
        /// `created`
        Created = "created",
        /// `updated`
        Updated = "updated",
    }
);

wire_enum!(
    /// `direction` of sorted listings.
    Direction {
        /// `asc`
        Asc = "asc",
        /// `desc`
        Desc = "desc",
    }
);

wire_enum!(
    /// `status` filter of the workflow-run listings. GitHub's documented enumeration mixes run
    /// statuses and conclusions; both are accepted by the same parameter.
    RunStatus {
        /// `completed`
        Completed = "completed",
        /// `action_required`
        ActionRequired = "action_required",
        /// `cancelled`
        Cancelled = "cancelled",
        /// `failure`
        Failure = "failure",
        /// `neutral`
        Neutral = "neutral",
        /// `skipped`
        Skipped = "skipped",
        /// `stale`
        Stale = "stale",
        /// `success`
        Success = "success",
        /// `timed_out`
        TimedOut = "timed_out",
        /// `in_progress`
        InProgress = "in_progress",
        /// `queued`
        Queued = "queued",
        /// `requested`
        Requested = "requested",
        /// `waiting`
        Waiting = "waiting",
        /// `pending`
        Pending = "pending",
    }
);

wire_enum!(
    /// `filter` of `GET .../actions/runs/{run_id}/jobs` (default `latest`: the latest attempt).
    JobFilter {
        /// `latest`
        Latest = "latest",
        /// `all` (every attempt)
        All = "all",
    }
);

wire_enum!(
    /// `archive_format` path value of the artifact download. GitHub documents `zip` only.
    ArchiveFormat {
        /// `zip`
        Zip = "zip",
    }
);

/// `per_page`: 1 through 100.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PerPage(u8);

impl PerPage {
    /// GitHub's maximum.
    pub const MAX: PerPage = PerPage(100);

    /// A page size; anything outside 1–100 is refused (GitHub would silently clamp it).
    ///
    /// # Errors
    ///
    /// [`ParamError::PerPage`].
    pub fn new(value: u8) -> Result<Self, ParamError> {
        if (1..=100).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ParamError::PerPage)
        }
    }

    /// The page size.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for PerPage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// An ISO 8601 UTC timestamp, `YYYY-MM-DDTHH:MM:SSZ`, as GitHub's `since` parameters document
/// and as its resources report `created_at`/`updated_at`. Kept as text: this SDK has no date
/// arithmetic, it only validates the shape and the field ranges (no leap-second or
/// days-per-month check beyond 1–31).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    /// Parses `YYYY-MM-DDTHH:MM:SSZ`.
    ///
    /// # Errors
    ///
    /// [`ParamError::Timestamp`].
    pub fn parse(text: &str) -> Result<Self, ParamError> {
        let b = text.as_bytes();
        let digits = |range: std::ops::Range<usize>| -> Option<u32> {
            let mut value = 0u32;
            for &c in &b[range] {
                if !c.is_ascii_digit() {
                    return None;
                }
                value = value * 10 + u32::from(c - b'0');
            }
            Some(value)
        };
        let shape = b.len() == 20
            && b[4] == b'-'
            && b[7] == b'-'
            && b[10] == b'T'
            && b[13] == b':'
            && b[16] == b':'
            && b[19] == b'Z';
        if !shape {
            return Err(ParamError::Timestamp);
        }
        let fields = (
            digits(0..4),
            digits(5..7),
            digits(8..10),
            digits(11..13),
            digits(14..16),
            digits(17..19),
        );
        match fields {
            (Some(_), Some(mo), Some(d), Some(h), Some(mi), Some(s))
                if (1..=12).contains(&mo)
                    && (1..=31).contains(&d)
                    && h < 24
                    && mi < 60
                    && s < 60 =>
            {
                Ok(Self(text.to_owned()))
            }
            _ => Err(ParamError::Timestamp),
        }
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `labels`: a non-empty list of label names, sent comma-separated (`bug,ui,@high`) as GitHub
/// documents. An issue matches when it has **all** of them.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Labels(Vec<String>);

impl Labels {
    /// The names, in order.
    ///
    /// # Errors
    ///
    /// [`ParamError::Labels`] for an empty list, an empty name or a name containing a comma (it
    /// would be split into two names on the wire).
    pub fn new<I, S>(names: I) -> Result<Self, ParamError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names: Vec<String> = names.into_iter().map(Into::into).collect();
        if names.is_empty() || names.iter().any(|n| n.is_empty() || n.contains(',')) {
            return Err(ParamError::Labels);
        }
        Ok(Self(names))
    }

    /// The names.
    pub fn names(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Display for Labels {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, name) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            f.write_str(name)?;
        }
        Ok(())
    }
}

/// The `X-GitHub-Api-Version` value. [`ApiVersion::PINNED`] is the version this SDK's models
/// were checked against (see `PROVIDER.md`); GitHub answers `410 Gone` for an unsupported
/// version and uses `2022-11-28` when the header is missing, which is why the SDK always sends
/// one.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ApiVersion(String);

impl ApiVersion {
    /// The pinned version text.
    pub const PINNED_TEXT: &'static str = "2026-03-10";

    /// The version the SDK was written and checked against.
    pub fn pinned() -> Self {
        Self(Self::PINNED_TEXT.to_owned())
    }

    /// Another `YYYY-MM-DD` version. The models may not match it; see `PROVIDER.md`.
    ///
    /// # Errors
    ///
    /// [`ParamError::ApiVersion`].
    pub fn new(date: &str) -> Result<Self, ParamError> {
        let b = date.as_bytes();
        let ok = b.len() == 10
            && b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit());
        if ok {
            Ok(Self(date.to_owned()))
        } else {
            Err(ParamError::ApiVersion)
        }
    }

    /// The text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ApiVersion {
    fn default() -> Self {
        Self::pinned()
    }
}

impl fmt::Display for ApiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An entity tag exactly as GitHub returned it in `ETag` (quotes and a `W/` prefix included),
/// sent back unchanged in `If-None-Match`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ETag(String);

impl ETag {
    /// An entity tag received earlier (for example one an application stored).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The text, unchanged.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ETag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
