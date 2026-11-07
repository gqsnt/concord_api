//! Walking a customer query with `STARTPOSITION`/`MAXRESULTS` (one-based offsets).

use std::num::NonZeroU32;

use concord_pagination::{OffsetError, Page, Pages, Resume, next_offset};

use crate::{Customer, CustomerPage, CustomerQuery, Error, PaginationError, QuickBooks};

/// Every customer `query` selects, page by page, starting at its `STARTPOSITION`.
///
/// Each page is the query with the next `STARTPOSITION`, sent with the generated `send()` (the
/// connection's policy). A page shorter than `MAXRESULTS` is the last; a full page is followed by
/// another request. A page that reports another `startPosition` or more customers than
/// `MAXRESULTS` stops the walk with [`Error::Pagination`]. `Ok(None)` from the traversal means the
/// last page was read; cap it with `max_pages`. Offsets are not a snapshot: order the query
/// ([`CustomerQuery::order_by_display_name`]) and expect shifts if customers change meanwhile.
pub fn customer_pages(
    qbo: &QuickBooks,
    query: CustomerQuery,
) -> Pages<Customer, NonZeroU32, (), Error> {
    let qbo = qbo.clone();
    Pages::new(Resume::Start, move |position: Option<NonZeroU32>| {
        let query = match position {
            Some(position) => query.clone().start_position(position),
            None => query.clone(),
        };
        let qbo = qbo.clone();
        async move {
            let response = qbo.customers().query(query.clone()).send().await?;
            page(&query, response.page)
        }
    })
}

fn page(query: &CustomerQuery, page: CustomerPage) -> Result<Page<Customer, NonZeroU32>, Error> {
    let start = query.position().get();
    if page
        .start_position
        .is_some_and(|reported| reported != start)
    {
        return Err(Error::Pagination(PaginationError::StartPositionMismatch));
    }
    let offset = |e| Error::Pagination(PaginationError::Offset(e));
    let next = next_offset(
        u64::from(start),
        u64::from(query.page_size()),
        page.customers.len() as u64,
        None,
    )
    .map_err(offset)?
    .map(|next| {
        u32::try_from(next)
            .ok()
            .and_then(NonZeroU32::new)
            .ok_or(offset(OffsetError::Overflow))
    })
    .transpose()?;
    Ok(Page::new(page.customers, next))
}
