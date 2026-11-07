//! Validated input and output for adding playlist items.

use serde::{Deserialize, Serialize};

use crate::Error;

/// One ordered batch of 1–100 track or episode URIs to add to a playlist.
///
/// Construct with [`Self::new`]; private fields keep generated body setters from bypassing
/// validation. Duplicate URIs are allowed and retain their order. Only catalog URIs of the
/// form `spotify:track:<id>` or `spotify:episode:<id>` are accepted, with a 22-character ASCII
/// base-62 ID. This checks syntax, not existence or the user's access to the item.
#[derive(Debug, Clone, Serialize)]
pub struct AddItems {
    uris: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position: Option<u32>,
}

impl AddItems {
    /// Validates a batch before a request (including a token refresh) can be sent.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for an empty batch, more than 100 items, or a malformed or
    /// unsupported URI. Batches are never split into multiple writes automatically.
    pub fn new<I, S>(uris: I) -> Result<Self, Error>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut items = Vec::new();
        for uri in uris {
            if items.len() == 100 {
                return Err(Error::InvalidInput(
                    "a playlist addition accepts at most 100 URIs",
                ));
            }
            let uri = uri.into();
            let id = uri
                .strip_prefix("spotify:track:")
                .or_else(|| uri.strip_prefix("spotify:episode:"));
            if !id.is_some_and(|id| {
                id.len() == 22 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
            }) {
                return Err(Error::InvalidInput(
                    "an item URI must be spotify:track: or spotify:episode: followed by a 22-character base-62 ID",
                ));
            }
            items.push(uri);
        }
        if items.is_empty() {
            return Err(Error::InvalidInput(
                "a playlist addition needs at least one URI",
            ));
        }
        Ok(Self {
            uris: items,
            position: None,
        })
    }

    /// Insert at this zero-based position; without this setter the items are appended.
    /// Spotify validates the position against the current playlist length.
    #[must_use]
    pub fn position(mut self, position: u32) -> Self {
        self.position = Some(position);
        self
    }
}

/// The new playlist version returned by a successful addition (`201 Created`).
#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistSnapshot {
    /// Opaque playlist version; it is not a Spotify catalog ID.
    pub snapshot_id: String,
}
