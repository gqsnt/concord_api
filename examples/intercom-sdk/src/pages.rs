use std::{future::Future, num::NonZeroUsize};

use concord_pagination::{Page, Pages, Resume};

use crate::{Contact, ContactList, Error, IntercomContactsList, IntercomContactsSearch, Response};

/// Page metadata retained beside contacts; the count never supplies the next cursor.
#[derive(Debug)]
pub struct PageMeta {
    /// Provider's advertised count, which can change during traversal.
    pub total_count: u64,
    /// Status, operation and headers. Explicit header access is unredacted.
    pub response: concord::ResponseMeta,
}

/// Bounded contacts traversal; failures retain their typed SDK error and failed cursor.
pub type ContactPages = Pages<Contact, String, PageMeta, Error>;

impl IntercomContactsList {
    /// Traverse from the selected resume point, preserving this operation's page size.
    /// `from` replaces any `starting_after` on the template. A nonzero cap is mandatory.
    /// Resume points belong to the same credentials, base, version and page size.
    /// Empty pages continue when `pages.next` exists. No page is retried automatically.
    pub fn pages(self, from: Resume<String>, max_pages: NonZeroUsize) -> ContactPages {
        contact_pages(from, max_pages, move |cursor| {
            self.clone().maybe_starting_after(cursor).send()
        })
    }
}

impl IntercomContactsSearch {
    /// Traverse exact-email matches, preserving this operation's email and page size.
    /// `from` replaces the template body's cursor. A nonzero cap is mandatory.
    /// Resume points belong to the same email, credentials, base, version and page size.
    /// Empty pages continue when `pages.next` exists. No page is retried automatically.
    pub fn pages(self, from: Resume<String>, max_pages: NonZeroUsize) -> ContactPages {
        contact_pages(from, max_pages, move |cursor| {
            let mut op = self.clone();
            op.search = op.search.with_starting_after(cursor);
            op.send()
        })
    }
}

// List and search share the response envelope, cursor validation and traversal policy.
// Only their request cursor placement differs.
fn contact_pages<F, Fut>(
    from: Resume<String>,
    max_pages: NonZeroUsize,
    mut fetch: F,
) -> ContactPages
where
    F: FnMut(Option<String>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Response<ContactList>, Error>> + Send + 'static,
{
    Pages::new(from, move |cursor: Option<String>| {
        let response = fetch(cursor.clone());
        async move {
            let response = response.await?;
            let list = response.value;
            let next = list.pages.and_then(|p| p.next).map(|n| n.starting_after);
            if next
                .as_ref()
                .is_some_and(|n| n.is_empty() || Some(n) == cursor.as_ref())
            {
                return Err(Error::InvalidCursor(Box::new(response.meta)));
            }
            Ok(Page {
                items: list.data,
                next,
                meta: PageMeta {
                    total_count: list.total_count,
                    response: response.meta,
                },
            })
        }
    })
    .max_pages(max_pages)
}
