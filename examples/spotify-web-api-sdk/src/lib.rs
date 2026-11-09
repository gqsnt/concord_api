//! A bounded Spotify Web API SDK on Concord.
//!
//! Two connected workflows, tested offline against local fixtures:
//!
//! 1. search the catalog for artists ([`WebApi::search_artists`]) and retrieve the one selected
//!    ([`WebApi::artist`]);
//! 2. list the authorized user's playlists ([`WebApiMe::playlists`]) and fetch the one selected
//!    ([`WebApiPlaylist::get`], continued with [`WebApiPlaylist::items`]), and add items to an
//!    owned playlist ([`WebApiPlaylist::add_items`]).
//!
//! Authorization is the authorization-code flow with refresh ([`OAuth`], [`Session`]). The
//! session is the client's send policy: every generated `send()` obtains an access token
//! (shared single-flight refresh), applies it, replays reads once after a 401, and returns this
//! crate's [`Error`] on failure. Writes are sent once; [`Error::UncertainWrite`] requires re-reading the
//! playlist before deciding what to do next. `prepare()` builds the declared request
//! **without** `Authorization`.
//!
//! ```no_run
//! use concord_sdk_spotify_web_api::{
//!     AppCredentials, AuthorizationCode, ClientSecret, OAuth, SessionOptions, WebApi, WebApiConfig,
//! };
//! # async fn demo(code: AuthorizationCode) -> Result<(), Box<dyn std::error::Error>> {
//! let http = concord::reqwest::Client::new();
//! let app = AppCredentials::new("client-id", &ClientSecret::new("client-secret"))?;
//! let oauth = OAuth::new(http.clone(), app);
//! let grant = oauth.exchange_code(&code, "https://app.example/callback").await?;
//! let session = oauth.session(grant, SessionOptions::new());
//! let api = WebApi::new(http, WebApiConfig { session: session.clone() });
//!
//! let found = api.search_artists("Nina Simone").set_limit(5).send().await?;
//! let artist = api.artist(found.artists.items[0].id.clone()).send().await?;
//! let playlists = api.me().playlists().send().await?;
//! let playlist = api.playlist(playlists.items[0].id.clone()).get().send().await?;
//! // Persist the refresh token: it may have been rotated.
//! let _stored = session.refresh_token();
//! # let _ = (artist, playlist); Ok(()) }
//! ```

#![warn(missing_docs)]

mod add_items;
mod auth;
mod error;
mod models;
mod pages;
mod session;

use concord::Json;

pub use add_items::{AddItems, PlaylistSnapshot};
pub use auth::{
    AppCredentials, AuthorizationCode, AuthorizeRequest, ClientSecret, Grant, OAuth, RefreshToken,
    State,
};
pub use concord;
/// The traversal types of [`pages`] (`Resume`, `PagesError`, `Incomplete`), re-exported so that
/// callers need no dependency of their own.
pub use concord_pagination;
pub use error::{
    ApiClass, ApiFailure, AuthError, Error, ErrorObject, OAuthErrorObject, TokenFailure,
};
pub use models::{
    Artist, ArtistSearch, Episode, ExternalUrls, Followers, Image, ItemCount, Market, Owner,
    Paging, Playable, Playlist, PlaylistItem, SimplifiedArtist, SimplifiedPlaylist, SpotifyId,
    Track,
};
pub use pages::{Listed, Listing, pages, pages_from};
pub use session::{ERROR_BODY_LIMIT, Session, SessionOptions};

/// Successful documents are read up to this many bytes (a playlist with its first 100 items fits
/// many times over).
pub const BODY_LIMIT: usize = 4 * 1024 * 1024;

concord::api! {
    /// The Spotify Web API (`https://api.spotify.com/v1`), as far as this SDK's slice goes.
    ///
    /// Every `send()` goes through the [`Session`]: it authorizes the request, refreshes and
    /// replays reads once after a 401, and maps non-2xx answers to [`Error::Api`]. Writes are
    /// never replayed, even after a 401 or a retryable 429.
    pub client WebApi(session: Session) {
        base "https://api.spotify.com";
        path "v1";
        format Json;
        limit BODY_LIMIT;
        send session -> Result<_, Error>;

        /// The page parameters of every offset listing: `limit` (page size) and `offset` (index
        /// of the first item). Absent, the provider's defaults apply.
        params PageParams(query limit?: u32, query offset?: u32);

        /// Searches the catalog for artists (`GET /v1/search?type=artist`).
        GET search_artists(query q: String, query market?: Market, ..PageParams) -> ArtistSearch {
            path "search";
            query { "type" = "artist"; }
        }

        /// One artist (`GET /v1/artists/{id}`).
        GET artist(id: SpotifyId) -> Artist { path "artists/{id}"; }

        /// The authorized user's resources (`/v1/me`).
        scope me {
            path "me";

            /// The playlists the user owns or follows (`GET /v1/me/playlists`). Private
            /// playlists need the `playlist-read-private` scope, collaborative ones
            /// `playlist-read-collaborative`.
            GET playlists(..PageParams) -> Paging<SimplifiedPlaylist> { path "playlists"; }
        }

        /// One playlist (`/v1/playlists/{id}`).
        scope playlist(id: SpotifyId) {
            path "playlists/{id}";

            /// The playlist with its first page of items (`GET /v1/playlists/{id}`).
            GET get(query market?: Market) -> Playlist {}

            /// A page of the playlist's items (`GET /v1/playlists/{id}/tracks`).
            GET items(query market?: Market, ..PageParams) -> Paging<PlaylistItem> {
                path "tracks";
            }

            /// Add up to 100 items to an owned playlist (`POST /v1/playlists/{id}/tracks`).
            /// Requires `playlist-modify-public` or `playlist-modify-private` as appropriate.
            /// Returns the new snapshot ID. Never replayed automatically; on
            /// [`Error::UncertainWrite`], re-read the playlist to reconcile the addition.
            ///
            /// Provider reference (read 2026-11-07):
            /// <https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist>.
            /// Spotify marks this route deprecated; it is retained for this SDK's contract.
            POST add_items(body items: AddItems) -> PlaylistSnapshot {
                path "tracks";
            }
        }
    }
}
