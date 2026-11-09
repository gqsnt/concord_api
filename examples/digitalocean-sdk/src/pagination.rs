use crate::{
    Action, Actions, DigitalOcean, Droplet, Droplets, Error, PageSize, Response, ResponseMeta,
    Snapshot, Snapshots, TagName,
};
use concord::{NextUrl, Sends, UrlContinuation, reqwest::Url};
use concord_pagination::{Page, Pages, Resume};
use std::{borrow::Cow, fmt, num::NonZeroUsize};

/// A validated opaque continuation. Debug never displays its URL.
/// It belongs to the original client, collection and query plan.
#[derive(Clone)]
pub struct NextLink(NextUrl);
impl fmt::Debug for NextLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextLink(<redacted>)")
    }
}

/// Bounded pages with explicit provider exhaustion, failure and incomplete outcomes.
pub type Traversal<T> = Pages<T, NextLink, ResponseMeta, Error>;

/// Query keys that select which records a listing contains. A followed or resumed link must
/// carry exactly the traversal's own values for each of them (none, for an unfiltered listing).
const FILTERS: [&str; 1] = ["tag_name"];

impl DigitalOcean {
    /// Walk droplets using `links.pages.next`. A cap with more pages is an error.
    pub fn droplet_pages(&self, size: PageSize, max_pages: NonZeroUsize) -> Traversal<Droplet> {
        pages(self.droplets().list().set_per_page(size), max_pages)
    }

    /// Walk only the droplets carrying `tag`. Every followed or resumed link must keep the tag.
    pub fn tagged_droplet_pages(
        &self,
        tag: &TagName,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Droplet> {
        let list = self.droplets().list().set_per_page(size);
        pages(list.set_tag_name(tag.clone()), max_pages)
    }

    /// Walk the selected droplet's actions with the same trust and completion rules.
    pub fn action_pages(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Action> {
        let list = self.droplet(droplet_id).actions().set_per_page(size);
        pages(list, max_pages)
    }

    /// Walk the selected droplet's snapshots and backups with the same rules.
    pub fn snapshot_pages(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Snapshot> {
        let list = self.droplet(droplet_id).snapshots().set_per_page(size);
        pages(list, max_pages)
    }
}

trait Listing: Send + 'static {
    type Item: Send + 'static;
    fn items(self) -> Vec<Self::Item>;
}
impl Listing for Droplets {
    type Item = Droplet;
    fn items(self) -> Vec<Droplet> {
        self.droplets
    }
}
impl Listing for Actions {
    type Item = Action;
    fn items(self) -> Vec<Action> {
        self.actions
    }
}
impl Listing for Snapshots {
    type Item = Snapshot;
    fn items(self) -> Vec<Snapshot> {
        self.snapshots
    }
}

fn pages<L, E>(template: E, max_pages: NonZeroUsize) -> Traversal<L::Item>
where
    L: Listing,
    E: UrlContinuation<L> + Sends<Result<Response<L>, Error>> + Clone + Send + 'static,
{
    Pages::new(Resume::Start, move |cursor: Option<NextLink>| {
        let endpoint = template.clone();
        async move {
            let prepared = endpoint.clone().prepare()?;
            let collection = prepared.request().url().clone();
            let response = match cursor {
                None => endpoint.send().await?,
                Some(cursor) => {
                    // Revalidate against this template, even for an already validated key.
                    validate(&collection, cursor.0.expose())?;
                    endpoint.follow(cursor.0).send().await?
                }
            };
            // The declared continuation, read once the policy accepted the page: an invalid one
            // refuses this whole page before its items escape.
            let next = E::next_url(&response.value).map_err(|_| Error::UnsafeNext)?;
            let next = next
                .map(|next| validate(&collection, next.expose()))
                .transpose()?;
            Ok(Page {
                items: response.value.items(),
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
    Ok(NextLink(NextUrl::from_url(url)))
}

/// Decoded values of one query key, in order, so `env%3Aprod` and `env:prod` compare equal.
fn values<'a>(url: &'a Url, key: &str) -> Vec<Cow<'a, str>> {
    url.query_pairs()
        .filter(|(name, _)| name == key)
        .map(|(_, value)| value)
        .collect()
}
