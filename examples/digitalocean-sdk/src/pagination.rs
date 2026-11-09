use crate::{
    Action, Actions, DigitalOcean, Droplet, Droplets, Error, Links, PageSize, Policy, Response,
    ResponseMeta, Snapshot, Snapshots, TagName,
};
use concord::{Endpoint, Sends, reqwest::Url};
use concord_pagination::{Page, Pages, Resume};
use std::{borrow::Cow, num::NonZeroUsize};

/// A validated opaque continuation. Debug never displays its URL.
/// It belongs to the original client, collection and query plan.
#[derive(Clone, concord::Debug)]
pub struct NextLink(#[sensitive] String);

/// Bounded pages with explicit provider exhaustion, failure and incomplete outcomes.
pub type Traversal<T> = Pages<T, NextLink, ResponseMeta, Error>;

/// Query keys that select which records a listing contains. A followed or resumed link must
/// carry exactly the traversal's own values for each of them (none, for an unfiltered listing).
const FILTERS: [&str; 1] = ["tag_name"];

impl DigitalOcean {
    /// Walk droplets using `links.pages.next`. A cap with more pages is an error.
    pub fn droplet_pages(&self, size: PageSize, max_pages: NonZeroUsize) -> Traversal<Droplet> {
        self.droplet_pages_from(size, max_pages, Resume::Start)
    }

    /// Continue a droplet walk from the position a previous one reported (`PagesError::resume`).
    ///
    /// `size` applies to `Resume::Start`; a resumed link keeps the page size it was issued with.
    /// The link is checked again, before any request, against this collection and its filter. Use `concord_sdk_digitalocean::concord_pagination::Resume` for the position. `Resume::Start` repeats the first droplets request with `size`; `Resume::At(key)` takes the `NextLink` from `links.pages.next` in a returned page's `next`.
    pub fn droplet_pages_from(
        &self,
        size: PageSize,
        max_pages: NonZeroUsize,
        from: Resume<NextLink>,
    ) -> Traversal<Droplet> {
        pages(self.droplets().list().set_per_page(size), from, max_pages)
    }

    /// Walk only the droplets carrying `tag`. Every followed or resumed link must keep the tag.
    pub fn tagged_droplet_pages(
        &self,
        tag: &TagName,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Droplet> {
        self.tagged_droplet_pages_from(tag, size, max_pages, Resume::Start)
    }

    /// Continue a tagged droplet walk; a link without exactly this tag is refused unsent. Use `concord_sdk_digitalocean::concord_pagination::Resume` for the position. `Resume::Start` repeats the first droplets request with `tag` and `size`; `Resume::At(key)` takes the `NextLink` from `links.pages.next` in a returned page's `next`.
    pub fn tagged_droplet_pages_from(
        &self,
        tag: &TagName,
        size: PageSize,
        max_pages: NonZeroUsize,
        from: Resume<NextLink>,
    ) -> Traversal<Droplet> {
        let list = self.droplets().list().set_per_page(size);
        pages(list.set_tag_name(tag.clone()), from, max_pages)
    }

    /// Walk the selected droplet's actions with the same trust and completion rules.
    pub fn action_pages(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Action> {
        self.action_pages_from(droplet_id, size, max_pages, Resume::Start)
    }

    /// Continue an action walk of the same droplet. Use `concord_sdk_digitalocean::concord_pagination::Resume` for the position. `Resume::Start` repeats the droplet's first actions request with `size`; `Resume::At(key)` takes the `NextLink` from `links.pages.next` in a returned page's `next`.
    pub fn action_pages_from(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
        from: Resume<NextLink>,
    ) -> Traversal<Action> {
        let list = self.droplet(droplet_id).actions().set_per_page(size);
        pages(list, from, max_pages)
    }

    /// Walk the selected droplet's snapshots and backups with the same rules.
    pub fn snapshot_pages(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Snapshot> {
        self.snapshot_pages_from(droplet_id, size, max_pages, Resume::Start)
    }

    /// Continue a snapshot walk of the same droplet. Use `concord_sdk_digitalocean::concord_pagination::Resume` for the position. `Resume::Start` repeats the droplet's first snapshots request with `size`; `Resume::At(key)` takes the `NextLink` from `links.pages.next` in a returned page's `next`.
    pub fn snapshot_pages_from(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
        from: Resume<NextLink>,
    ) -> Traversal<Snapshot> {
        let list = self.droplet(droplet_id).snapshots().set_per_page(size);
        pages(list, from, max_pages)
    }
}

trait Listing: Send + 'static {
    type Item: Send + 'static;
    fn split(self) -> (Vec<Self::Item>, Option<Links>);
}
impl Listing for Droplets {
    type Item = Droplet;
    fn split(self) -> (Vec<Droplet>, Option<Links>) {
        (self.droplets, self.links)
    }
}
impl Listing for Actions {
    type Item = Action;
    fn split(self) -> (Vec<Action>, Option<Links>) {
        (self.actions, self.links)
    }
}
impl Listing for Snapshots {
    type Item = Snapshot;
    fn split(self) -> (Vec<Snapshot>, Option<Links>) {
        (self.snapshots, self.links)
    }
}

fn pages<L, E>(template: E, from: Resume<NextLink>, max_pages: NonZeroUsize) -> Traversal<L::Item>
where
    L: Listing,
    E: Endpoint<L> + Sends<Result<Response<L>, Error>> + Clone + Send + 'static,
{
    Pages::new(from, move |cursor: Option<NextLink>| {
        let endpoint = template.clone();
        async move {
            let prepared = endpoint.clone().prepare()?;
            let collection = prepared.request().url().clone();
            let response = match cursor {
                None => endpoint.send().await?,
                Some(cursor) => {
                    // Revalidate against this template, even for an already validated key.
                    validate(&collection, &cursor.0)?;
                    Policy::finish(prepared.follow(&cursor.0)?.receive().await?).await?
                }
            };
            let (items, links) = response.value.split();
            let next = links
                .and_then(|links| links.pages)
                .and_then(|pages| pages.next)
                .map(|next| validate(&collection, &next))
                .transpose()?;
            Ok(Page {
                items,
                next,
                meta: response.meta,
            })
        }
    })
    .max_pages(max_pages)
}

fn validate(collection: &Url, target: &str) -> Result<NextLink, Error> {
    // Absolute provider links only. Reject parser-repaired control characters and userinfo.
    if target.chars().any(char::is_control) || target.contains('\\') {
        return Err(Error::UnsafeNext);
    }
    let url = Url::parse(target).map_err(|_| Error::UnsafeNext)?;
    if url.origin() != collection.origin()
        || url.path() != collection.path()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || FILTERS
            .iter()
            .any(|key| values(&url, key) != values(collection, key))
    {
        return Err(Error::UnsafeNext);
    }
    Ok(NextLink(url.into()))
}

/// Decoded values of one query key, in order, so `env%3Aprod` and `env:prod` compare equal.
fn values<'a>(url: &'a Url, key: &str) -> Vec<Cow<'a, str>> {
    url.query_pairs()
        .filter(|(name, _)| name == key)
        .map(|(_, value)| value)
        .collect()
}
