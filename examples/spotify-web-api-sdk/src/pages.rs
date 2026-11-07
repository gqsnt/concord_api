//! Offset traversal of the provider's listings, on `concord-pagination`.
//!
//! One adapter serves every listing of the declaration: the endpoints that include the
//! `PageParams` group (`limit`, `offset`). The provider's `next` link only says whether a next
//! page exists; the next request is the same endpoint with `offset = offset + limit`, and every
//! page the SDK requested must start at the offset it asked for.

use concord::Sends;
use concord_pagination::{Page, Pages, Resume};

use crate::models::{ArtistSearch, Paging};
use crate::{Error, PageParams};

/// A provider response that carries one listing.
pub trait Listing: Send + 'static {
    /// The items of the listing.
    type Item: Send + 'static;

    /// Moves the paging object out of the response.
    fn into_paging(self) -> Paging<Self::Item>;
}

impl<T: Send + 'static> Listing for Paging<T> {
    type Item = T;

    fn into_paging(self) -> Paging<T> {
        self
    }
}

impl Listing for ArtistSearch {
    type Item = crate::models::Artist;

    fn into_paging(self) -> Paging<Self::Item> {
        self.artists
    }
}

/// The traversal of a listing, keyed by offset. `Ok(None)` from it means the provider's last
/// page was read; a failed page is `PagesError::Provider` with that page's offset to resume
/// from ([`pages_from`]).
pub type Listed<T> = Pages<T, u32, (), Error>;

/// Walks a listing from the request as given (its own `limit` and `offset`, if set).
///
/// ```no_run
/// # async fn demo(api: concord_sdk_spotify_web_api::WebApi) -> Result<(), Box<dyn std::error::Error>> {
/// use concord_sdk_spotify_web_api::pages;
/// let playlists = pages(api.me().playlists().set_limit(50))
///     .max_pages(std::num::NonZeroUsize::new(20).unwrap())
///     .items()
///     .collect_complete()
///     .await
///     .map_err(|incomplete| incomplete.error)?;
/// # let _ = playlists; Ok(()) }
/// ```
pub fn pages<E, R>(first: E) -> Listed<R::Item>
where
    E: PageParams + Sends<Result<R, Error>> + Clone + Send + 'static,
    R: Listing,
{
    pages_from(first, Resume::Start)
}

/// Walks a listing from `from`: `Resume::At(offset)` requests that offset first (a resume point
/// of a failed traversal, or a playlist's [`Paging::next_offset`]).
pub fn pages_from<E, R>(first: E, from: Resume<u32>) -> Listed<R::Item>
where
    E: PageParams + Sends<Result<R, Error>> + Clone + Send + 'static,
    R: Listing,
{
    Pages::new(from, move |offset: Option<u32>| {
        let request = match offset {
            Some(offset) => first.clone().set_offset(offset),
            None => first.clone(),
        };
        async move {
            let paging = request.send().await?.into_paging();
            if offset.is_some_and(|requested| requested != paging.offset) {
                return Err(Error::Protocol(
                    "the page does not start at the requested offset",
                ));
            }
            let next = paging.next_offset()?;
            Ok(Page::new(paging.items, next))
        }
    })
}
