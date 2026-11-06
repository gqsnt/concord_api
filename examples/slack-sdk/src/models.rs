//! Slack objects and short-lived pagination cursors.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Deserializer};

/// An opaque, short-lived Slack cursor. Use only with the same listing and options.
///
/// No serialization or persistent store is supplied: Slack says cursors expire and must not
/// be saved for hours or days. `Debug` redacts it; [`Self::as_str`] is an explicit raw access.
#[derive(Clone, PartialEq, Eq)]
pub struct Cursor(String);

impl Cursor {
    /// The exact provider string, without trimming, decoding, or pre-encoding.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Cursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Cursor(<redacted>)")
    }
}

fn cursor<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Cursor>, D::Error> {
    Ok(Option::<String>::deserialize(d)?
        .filter(|value| !value.is_empty())
        .map(Cursor))
}

/// Slack's body metadata. A missing, null, or empty cursor means exhaustion.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ResponseMetadata {
    /// Exact continuation for the next request, if any.
    #[serde(default, deserialize_with = "cursor")]
    pub next_cursor: Option<Cursor>,
    /// Provider messages, when supplied.
    #[serde(default)]
    pub messages: Vec<String>,
    /// Provider warnings, when supplied.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Unmodeled body metadata, retained for inspection.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A Slack user. Unmodeled fields, including `profile`, remain in [`Self::extra`].
#[derive(Clone, Debug, Deserialize)]
pub struct User {
    /// Slack user ID.
    pub id: String,
    /// User name, if visible.
    pub name: Option<String>,
    /// Workspace ID, if supplied.
    pub team_id: Option<String>,
    /// Whether the account is deactivated.
    pub deleted: Option<bool>,
    /// Real name, if visible to this token.
    pub real_name: Option<String>,
    /// Locale, when requested and available.
    pub locale: Option<String>,
    /// Fields outside this small SDK's projection.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// One successful `users.list` page; continuation lives in [`crate::Response::response_metadata`].
#[derive(Debug)]
pub struct UsersPage {
    /// Users on this page; an empty vector alone does not indicate exhaustion.
    pub members: Vec<User>,
    /// Provider cache timestamp, when present.
    pub cache_ts: Option<u64>,
}

/// A conversation: public/private channel, direct message, or multi-party direct message.
/// Optional fields account for the different shapes; the ID is always required.
#[derive(Clone, Debug, Deserialize)]
pub struct Conversation {
    /// Conversation ID.
    pub id: String,
    /// Channel name (often absent on direct messages).
    pub name: Option<String>,
    /// Channel flag.
    pub is_channel: Option<bool>,
    /// Direct-message flag.
    pub is_im: Option<bool>,
    /// Multi-party direct-message flag.
    pub is_mpim: Option<bool>,
    /// Private-channel flag.
    pub is_private: Option<bool>,
    /// Archived flag.
    pub is_archived: Option<bool>,
    /// Member count, when requested and available.
    pub num_members: Option<u64>,
    /// Locale, when requested and available.
    pub locale: Option<String>,
    /// Other fields, including `topic`, `purpose`, and DM-specific fields.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}
