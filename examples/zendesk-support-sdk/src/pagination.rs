//! Cursor traversal of List Tickets over `concord-pagination`.

use std::num::NonZeroUsize;

use concord::reqwest::Url;
use concord_pagination::{Page, Pages, Resume};

use crate::{Error, PaginationError, Ticket, TicketPage, ZendeskTicketsList};

/// Every page of a List Tickets request, in order, each sent through the client's
/// [`SendPolicy`](crate::SendPolicy) (so a rate-limited page honours `Retry-After`).
///
/// `template` fixes the page size, sort and filters; its `page[after]` is replaced by the
/// traversal (`from`: `Resume::Start` for the first page, `Resume::At(cursor)` to continue from a
/// cursor this traversal returned for the same template). Each next page is the template with
/// `page[after]` set to the previous page's `meta.after_cursor`; the provider's `links.next`, when
/// present, must name that same cursor.
///
/// The listing ends exactly when a page says `meta.has_more: false`. More than `max_pages`
/// requests while the provider still has pages is `PagesError::Bounded` with the cursor to resume
/// from, never a silently shortened list. A page that says it has more but gives no usable
/// cursor, or the cursor that produced it, is [`Error::Pagination`].
pub fn ticket_pages(
    template: ZendeskTicketsList,
    from: Resume<String>,
    max_pages: NonZeroUsize,
) -> Pages<Ticket, String, (), Error> {
    Pages::new(from, move |cursor: Option<String>| {
        let request = template.clone().maybe_after(cursor.clone());
        async move {
            let page = request.send().await?;
            let next = next_cursor(&page, cursor.as_deref())?;
            Ok(Page::new(page.tickets, next))
        }
    })
    .max_pages(max_pages)
}

fn next_cursor(page: &TicketPage, current: Option<&str>) -> Result<Option<String>, Error> {
    if !page.meta.has_more {
        return Ok(None);
    }
    let cursor = match page.meta.after_cursor.as_deref() {
        Some(cursor) if !cursor.is_empty() => cursor,
        _ => return Err(Error::Pagination(PaginationError::MissingCursor)),
    };
    if current == Some(cursor) {
        return Err(Error::Pagination(PaginationError::RepeatedCursor));
    }
    if let Some(next) = page.links.next.as_deref() {
        let named = Url::parse(next).ok().and_then(|url| {
            url.query_pairs()
                .find(|(key, _)| key == "page[after]")
                .map(|(_, value)| value.into_owned())
        });
        if named.as_deref() != Some(cursor) {
            return Err(Error::Pagination(PaginationError::InconsistentNextLink));
        }
    }
    Ok(Some(cursor.to_owned()))
}
