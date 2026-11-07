use crate::{
    Action, Actions, DigitalOcean, Droplet, Droplets, Error, Links, PageSize, Policy, Response,
    ResponseMeta,
};
use concord::{Endpoint, Sends, reqwest::Url};
use concord_pagination::{Page, Pages, Resume};
use std::{fmt, num::NonZeroUsize};

/// A validated opaque continuation. Debug never displays its URL.
/// It belongs to the original client, collection and query plan.
#[derive(Clone)]
pub struct NextLink(String);
impl fmt::Debug for NextLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NextLink(<redacted>)")
    }
}

/// Bounded pages with explicit provider exhaustion, failure and incomplete outcomes.
pub type Traversal<T> = Pages<T, NextLink, ResponseMeta, Error>;

impl DigitalOcean {
    /// Walk droplets using `links.pages.next`. A cap with more pages is an error.
    pub fn droplet_pages(&self, size: PageSize, max_pages: NonZeroUsize) -> Traversal<Droplet> {
        pages(self.droplets().list().set_per_page(size), max_pages)
    }

    /// Walk the selected droplet's actions with the same trust and completion rules.
    pub fn action_pages(
        &self,
        droplet_id: u64,
        size: PageSize,
        max_pages: NonZeroUsize,
    ) -> Traversal<Action> {
        pages(
            self.droplet(droplet_id).actions().set_per_page(size),
            max_pages,
        )
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

fn pages<L, E>(template: E, max_pages: NonZeroUsize) -> Traversal<L::Item>
where
    L: Listing,
    E: Endpoint<L> + Sends<Result<Response<L>, Error>> + Clone + Send + 'static,
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
    {
        return Err(Error::UnsafeNext);
    }
    Ok(NextLink(url.into()))
}
