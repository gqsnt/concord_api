//! The provider's objects, as far as the two workflows read them.
//!
//! Fields that a development-mode app or a given account may not receive (popularity, follower
//! counts, genres, images) are optional or default to empty: the SDK never assumes extended quota
//! access. Unknown fields are ignored.

use std::fmt;

use serde::{Deserialize, Deserializer};

use crate::Error;

/// A Spotify catalog identifier (artist, playlist, track, episode): base-62 text.
///
/// [`SpotifyId::new`] accepts 1 to 64 ASCII letters and digits, so a URI (`spotify:artist:..`) or
/// a URL passed by mistake is refused before any request is built. The same rule applies when an
/// identifier is decoded from a response. User identifiers are not `SpotifyId`s (they are free
/// text); see [`Owner::id`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct SpotifyId(String);

impl SpotifyId {
    /// Validates a base-62 identifier.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] when the value is empty, longer than 64 bytes or contains anything
    /// but ASCII letters and digits.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Err(Error::InvalidInput(
                "a Spotify ID is 1 to 64 ASCII letters and digits",
            ));
        }
        Ok(Self(id))
    }

    /// The identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SpotifyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for SpotifyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SpotifyId({:?})", self.0)
    }
}

impl<'de> Deserialize<'de> for SpotifyId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(|_| serde::de::Error::custom("invalid Spotify ID"))
    }
}

/// A market (`market` query parameter): an ISO 3166-1 alpha-2 country code, or the market of the
/// user the access token belongs to.
#[derive(Clone, PartialEq, Eq)]
pub struct Market(MarketRepr);

#[derive(Clone, PartialEq, Eq)]
enum MarketRepr {
    FromToken,
    Country([u8; 2]),
}

impl Market {
    /// The user's own market (`market=from_token`).
    pub fn from_token() -> Self {
        Self(MarketRepr::FromToken)
    }

    /// A two-letter country code, sent uppercase (`se` is sent as `SE`).
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] unless the code is exactly two ASCII letters.
    pub fn country(code: &str) -> Result<Self, Error> {
        match code.as_bytes() {
            [a, b] if a.is_ascii_alphabetic() && b.is_ascii_alphabetic() => {
                Ok(Self(MarketRepr::Country([
                    a.to_ascii_uppercase(),
                    b.to_ascii_uppercase(),
                ])))
            }
            _ => Err(Error::InvalidInput("a market is a two-letter country code")),
        }
    }
}

impl fmt::Display for Market {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            MarketRepr::FromToken => f.write_str("from_token"),
            MarketRepr::Country([a, b]) => write!(f, "{}{}", char::from(*a), char::from(*b)),
        }
    }
}

impl fmt::Debug for Market {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Market({self})")
    }
}

/// An offset/limit page of a listing (the provider's paging object).
///
/// `next` is present while the provider has a following page; the SDK's traversal reads only its
/// presence and requests `offset + limit` next ([`Paging::next_offset`]).
#[derive(Debug, Clone, Deserialize)]
pub struct Paging<T> {
    /// The URL of this page.
    pub href: String,
    /// The items of this page.
    pub items: Vec<T>,
    /// The page size the provider applied.
    pub limit: u32,
    /// The URL of the next page, absent on the last page.
    pub next: Option<String>,
    /// The index of this page's first item in the whole listing.
    pub offset: u32,
    /// The URL of the previous page, absent on the first page.
    pub previous: Option<String>,
    /// The number of items in the whole listing.
    pub total: u32,
}

impl<T> Paging<T> {
    /// The offset of the following page (`offset + limit`), or `None` on the last page.
    ///
    /// # Errors
    ///
    /// [`Error::Protocol`] when the provider announces a next page with a zero `limit` (no
    /// progress is possible) or `offset + limit` overflows.
    pub fn next_offset(&self) -> Result<Option<u32>, Error> {
        if self.next.is_none() {
            return Ok(None);
        }
        if self.limit == 0 {
            return Err(Error::Protocol("a page with a next link has limit 0"));
        }
        self.offset
            .checked_add(self.limit)
            .map(Some)
            .ok_or(Error::Protocol("the next page offset overflows"))
    }
}

/// The search response for `type=artist`.
#[derive(Debug, Clone, Deserialize)]
pub struct ArtistSearch {
    /// The matching artists.
    pub artists: Paging<Artist>,
}

/// A link to the object on open.spotify.com.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ExternalUrls {
    /// The Spotify web player URL.
    pub spotify: Option<String>,
}

/// An image in several sizes.
#[derive(Debug, Clone, Deserialize)]
pub struct Image {
    /// The image URL.
    pub url: String,
    /// Height in pixels, when known.
    pub height: Option<u32>,
    /// Width in pixels, when known.
    pub width: Option<u32>,
}

/// A follower count.
#[derive(Debug, Clone, Deserialize)]
pub struct Followers {
    /// The number of followers.
    pub total: u64,
}

/// An artist (the full object of search results and `GET /v1/artists/{id}`).
#[derive(Debug, Clone, Deserialize)]
pub struct Artist {
    /// The artist's ID.
    pub id: SpotifyId,
    /// The artist's name.
    pub name: String,
    /// The Spotify URI (`spotify:artist:..`).
    pub uri: String,
    /// Genres; empty when not provided.
    #[serde(default, deserialize_with = "nullable_vec")]
    pub genres: Vec<String>,
    /// Popularity (0–100), when provided.
    #[serde(default)]
    pub popularity: Option<u32>,
    /// Followers, when provided.
    #[serde(default)]
    pub followers: Option<Followers>,
    /// Images; empty when not provided.
    #[serde(default, deserialize_with = "nullable_vec")]
    pub images: Vec<Image>,
    /// External links.
    #[serde(default)]
    pub external_urls: ExternalUrls,
}

/// An artist as embedded in a track. A local file's artists have no ID.
#[derive(Debug, Clone, Deserialize)]
pub struct SimplifiedArtist {
    /// The artist's ID; absent for local files.
    pub id: Option<SpotifyId>,
    /// The artist's name.
    pub name: String,
}

/// The owner of a playlist, or the user who added an item.
#[derive(Debug, Clone, Deserialize)]
pub struct Owner {
    /// The user's Spotify ID (free text, not base-62).
    pub id: String,
    /// The user's display name, when public.
    #[serde(default)]
    pub display_name: Option<String>,
}

/// The item count of a playlist in a listing.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemCount {
    /// The API URL of the playlist's items.
    pub href: String,
    /// The number of items.
    pub total: u32,
}

/// A playlist in the current user's listing (`GET /v1/me/playlists`).
#[derive(Debug, Clone, Deserialize)]
pub struct SimplifiedPlaylist {
    /// The playlist's ID.
    pub id: SpotifyId,
    /// The playlist's name.
    pub name: String,
    /// The description, when set.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the playlist is public; `None` when the provider does not say.
    #[serde(default)]
    pub public: Option<bool>,
    /// Whether other users may modify it.
    #[serde(default)]
    pub collaborative: bool,
    /// The owner.
    pub owner: Owner,
    /// The version identifier of the playlist.
    pub snapshot_id: String,
    /// Where its items are and how many there are, when provided.
    #[serde(default)]
    pub tracks: Option<ItemCount>,
    /// The Spotify URI (`spotify:playlist:..`).
    pub uri: String,
}

/// A full playlist (`GET /v1/playlists/{id}`), with the first page of its items.
#[derive(Debug, Clone, Deserialize)]
pub struct Playlist {
    /// The playlist's ID.
    pub id: SpotifyId,
    /// The playlist's name.
    pub name: String,
    /// The description, when set.
    #[serde(default)]
    pub description: Option<String>,
    /// Whether the playlist is public; `None` when the provider does not say.
    #[serde(default)]
    pub public: Option<bool>,
    /// Whether other users may modify it.
    #[serde(default)]
    pub collaborative: bool,
    /// The owner.
    pub owner: Owner,
    /// The version identifier of the playlist.
    pub snapshot_id: String,
    /// Followers, when provided.
    #[serde(default)]
    pub followers: Option<Followers>,
    /// The first page of items. Continue with [`Paging::next_offset`] and
    /// [`concord_pagination::Pages::starting_at`] over `playlist(id).items()`.
    pub tracks: Paging<PlaylistItem>,
    /// The Spotify URI (`spotify:playlist:..`).
    pub uri: String,
}

/// One entry of a playlist.
#[derive(Debug, Clone, Deserialize)]
pub struct PlaylistItem {
    /// When the item was added (ISO 8601, UTC); absent for very old playlists.
    #[serde(default)]
    pub added_at: Option<String>,
    /// Who added it, when known.
    #[serde(default)]
    pub added_by: Option<Owner>,
    /// Whether the entry is a local file.
    #[serde(default)]
    pub is_local: bool,
    /// The track or episode; `None` when the provider returns `null` (removed or unavailable).
    #[serde(default)]
    pub track: Option<Playable>,
}

/// What a playlist entry plays, by its `type` field.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Playable {
    /// A music track.
    Track(Track),
    /// A podcast episode.
    Episode(Episode),
    /// Any other type: kept as a marker, not decoded.
    #[serde(other)]
    Other,
}

/// A track in a playlist. A local file's track has no ID.
#[derive(Debug, Clone, Deserialize)]
pub struct Track {
    /// The track's ID; absent for local files.
    pub id: Option<SpotifyId>,
    /// The track's name.
    pub name: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
    /// The track's artists.
    #[serde(default, deserialize_with = "nullable_vec")]
    pub artists: Vec<SimplifiedArtist>,
    /// Whether the track is a local file.
    #[serde(default)]
    pub is_local: bool,
}

/// A podcast episode in a playlist.
#[derive(Debug, Clone, Deserialize)]
pub struct Episode {
    /// The episode's ID.
    pub id: SpotifyId,
    /// The episode's name.
    pub name: String,
    /// Duration in milliseconds.
    pub duration_ms: u64,
}

/// `null` and absent both decode to an empty vector.
fn nullable_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
