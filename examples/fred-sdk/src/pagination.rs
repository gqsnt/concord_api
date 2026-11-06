use std::num::NonZeroUsize;

use concord::Sends;
use concord_pagination::{Page, Pages, Resume};

use crate::{Error, Observation, Observations, OffsetOptions, SearchPage, Series};

/// An envelope with count/offset/limit and items, implemented by this slice's page models.
pub trait OffsetEnvelope<T> {
    /// Split the response into (count, offset, limit, items).
    fn into_offset_parts(self) -> (u64, u64, u64, Vec<T>);
}

impl OffsetEnvelope<Series> for SearchPage {
    fn into_offset_parts(self) -> (u64, u64, u64, Vec<Series>) {
        (self.count, self.offset, self.limit, self.seriess)
    }
}
impl OffsetEnvelope<Observation> for Observations {
    fn into_offset_parts(self) -> (u64, u64, u64, Vec<Observation>) {
        (self.count, self.offset, self.limit, self.observations)
    }
}

/// Walk a generated search or observations operation through its ordinary send policy.
///
/// `from` and the request template must refer to the same query, page size and real-time
/// snapshot. Offset 0 is used for `Resume::Start`; the template's offset is replaced.
/// The response must make progress and match its requested offset. A bounded stop is
/// a `concord_pagination::PagesError::Bounded`, never a silently complete prefix.
/// Counts can change when the provider data changes; use a stable provider snapshot when needed.
pub fn pages<E, R, T>(
    template: E,
    from: Resume<u64>,
    max_pages: NonZeroUsize,
) -> Pages<T, u64, (), Error>
where
    E: Sends<Result<R, Error>> + OffsetOptions + Clone + Send + 'static,
    R: OffsetEnvelope<T> + Send + 'static,
    T: Send + 'static,
{
    Pages::new(from, move |key: Option<u64>| {
        let offset = key.unwrap_or(0);
        let operation = template.clone().set_offset(offset);
        async move {
            let (count, actual, limit, items) = operation.send().await?.into_offset_parts();
            let len = items.len() as u64;
            let end = offset.checked_add(len).ok_or(Error::Pagination)?;
            if actual != offset
                || limit == 0
                || len > limit
                || end > count
                || (len == 0 && offset < count)
            {
                return Err(Error::Pagination);
            }
            Ok(Page::new(items, (end < count).then_some(end)))
        }
    })
    .max_pages(max_pages)
}
