//! Built-in policies for core's generic sequential pagination engine.

use std::num::{NonZeroU32, NonZeroU64};

use concord_core::pagination::{
    PageDecisionLimits, PageResponse, PageStep, PaginationController, PaginationDescription,
    PaginationError, PaginationErrorKind,
};

/// A decoded page that exposes an optional next cursor by reference.
///
/// The cursor and page may be sensitive. Implementations own logging and
/// storage and must keep synchronous CPU, memory, retries, maps, queues, tasks,
/// and metric cardinality finite. The callback must be non-blocking and perform
/// no external I/O.
pub trait HasNextCursor<C> {
    /// Returns the next cursor, distinguishing an absent cursor from a present
    /// cursor whose value is empty.
    fn next_cursor(&self) -> Option<&C>;
}

/// Checked offset pagination state.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct OffsetPagination {
    /// Current zero-based offset.
    pub offset: u64,
    /// Exact requested page size.
    pub page_size: NonZeroU32,
}

impl OffsetPagination {
    /// Creates offset pagination at `offset` with a positive page size.
    #[must_use]
    pub const fn new(offset: u64, page_size: NonZeroU32) -> Self {
        Self { offset, page_size }
    }

    /// Returns the current offset.
    #[must_use]
    pub const fn offset(&self) -> u64 {
        self.offset
    }

    /// Returns the requested page size.
    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }
}

impl<Page> PaginationController<Page> for OffsetPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_items(page.into_page(), limits)?;
        if items.len()
            < usize::try_from(self.page_size.get())
                .expect("u32 page sizes fit the supported usize targets")
        {
            return Ok(PageStep::Done { items });
        }
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            items,
            next: Self {
                offset,
                page_size: self.page_size,
            },
        })
    }
}

/// Checked one-based page-number pagination state.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PagePagination {
    /// Current one-based page number.
    pub page: NonZeroU64,
    /// Exact requested page size.
    pub page_size: NonZeroU32,
}

impl PagePagination {
    /// Creates page-number pagination at `page` with a positive page size.
    #[must_use]
    pub const fn new(page: NonZeroU64, page_size: NonZeroU32) -> Self {
        Self { page, page_size }
    }

    /// Returns the current page number.
    #[must_use]
    pub const fn page(&self) -> NonZeroU64 {
        self.page
    }

    /// Returns the requested page size.
    #[must_use]
    pub const fn page_size(&self) -> NonZeroU32 {
        self.page_size
    }
}

impl<Page> PaginationController<Page> for PagePagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn advance(
        self,
        response: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let items = collect_items(response.into_page(), limits)?;
        if items.len()
            < usize::try_from(self.page_size.get())
                .expect("u32 page sizes fit the supported usize targets")
        {
            return Ok(PageStep::Done { items });
        }
        let page = self
            .page
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(PageStep::Continue {
            items,
            next: Self {
                page,
                page_size: self.page_size,
            },
        })
    }
}

/// Typed cursor pagination state.
#[derive(Clone, Eq, PartialEq)]
pub struct CursorPagination<C> {
    /// Current optional cursor value.
    pub cursor: Option<C>,
}

/// Cursor pagination driven by the declared `X-Next-Cursor` response header.
#[derive(Clone, Eq, PartialEq)]
pub struct NextHeaderPagination {
    /// Current optional cursor bound into the next endpoint snapshot.
    pub cursor: Option<String>,
}

impl NextHeaderPagination {
    /// Creates header pagination with an optional initial cursor.
    #[must_use]
    pub const fn new(cursor: Option<String>) -> Self {
        Self { cursor }
    }
}

impl<Page> PaginationController<Page> for NextHeaderPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription =
        PaginationDescription::new(&["x-next-cursor"], false);

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let name = concord_core::contracts::HeaderName::from_static("x-next-cursor");
        let next = {
            let mut values = page.header_values(&name);
            let next = values
                .next()
                .map(|value| {
                    value
                        .to_str()
                        .map(str::to_owned)
                        .map_err(|_| PaginationError::new(PaginationErrorKind::InvalidContinuation))
                })
                .transpose()?;
            if values.next().is_some() {
                return Err(PaginationError::new(
                    PaginationErrorKind::InvalidContinuation,
                ));
            }
            next
        };
        let items = collect_items(page.into_page(), limits)?;
        Ok(match next {
            Some(cursor) => PageStep::Continue {
                items,
                next: Self::new(Some(cursor)),
            },
            None => PageStep::Done { items },
        })
    }
}

/// Same-origin RFC Link-header cursor pagination using the `cursor` query key.
#[derive(Clone, Eq, PartialEq)]
pub struct LinkCursorPagination {
    /// Current optional cursor bound into the next endpoint snapshot.
    pub cursor: Option<String>,
}

impl LinkCursorPagination {
    /// Creates Link pagination with an optional initial cursor.
    #[must_use]
    pub const fn new(cursor: Option<String>) -> Self {
        Self { cursor }
    }
}

impl<Page> PaginationController<Page> for LinkCursorPagination
where
    Page: IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription = PaginationDescription::new(&["link"], true);

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let current = page.current_uri().ok_or_else(|| {
            PaginationError::new(PaginationErrorKind::InvalidControllerDescription)
        })?;
        let name = concord_core::contracts::HeaderName::from_static("link");
        let values = page
            .header_values(&name)
            .map(|value| {
                value
                    .to_str()
                    .map_err(|_| PaginationError::new(PaginationErrorKind::InvalidContinuation))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = resolve_link_cursor(current, values)?;

        let items = collect_items(page.into_page(), limits)?;
        Ok(match next_cursor {
            Some(cursor) => PageStep::Continue {
                items,
                next: Self::new(Some(cursor)),
            },
            None => PageStep::Done { items },
        })
    }
}

fn resolve_link_cursor<'a>(
    current: &concord_core::contracts::Url,
    values: impl IntoIterator<Item = &'a str>,
) -> Result<Option<String>, PaginationError> {
    let mut next_cursor = None;
    for value in values {
        for entry in split_link_entries(value)? {
            let Some(target) = next_link_target(entry)? else {
                continue;
            };
            if next_cursor.is_some() {
                return Err(PaginationError::new(
                    PaginationErrorKind::InvalidContinuation,
                ));
            }
            let resolved = current
                .join(target)
                .map_err(|_| PaginationError::new(PaginationErrorKind::InvalidContinuation))?;
            if resolved.origin() != current.origin() {
                return Err(PaginationError::new(
                    PaginationErrorKind::CrossOriginContinuation,
                ));
            }
            let mut cursors = resolved
                .query_pairs()
                .filter(|(name, _)| name == "cursor")
                .map(|(_, value)| value.into_owned());
            let cursor = cursors
                .next()
                .ok_or_else(|| PaginationError::new(PaginationErrorKind::InvalidContinuation))?;
            if cursors.next().is_some() {
                return Err(PaginationError::new(
                    PaginationErrorKind::InvalidContinuation,
                ));
            }
            next_cursor = Some(cursor);
        }
    }
    Ok(next_cursor)
}

fn split_link_entries(value: &str) -> Result<Vec<&str>, PaginationError> {
    let mut entries = Vec::new();
    let mut start = 0_usize;
    let mut in_angle = false;
    let mut quoted = false;
    let mut escaped = false;
    for (index, byte) in value.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }
        match byte {
            b'\\' if quoted => escaped = true,
            b'"' if !in_angle => quoted = !quoted,
            b'<' if !quoted && !in_angle => in_angle = true,
            b'>' if !quoted && in_angle => in_angle = false,
            b',' if !quoted && !in_angle => {
                entries.push(value[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    if escaped || quoted || in_angle {
        return Err(PaginationError::new(
            PaginationErrorKind::InvalidContinuation,
        ));
    }
    entries.push(value[start..].trim());
    Ok(entries)
}

fn next_link_target(entry: &str) -> Result<Option<&str>, PaginationError> {
    let Some(target_end) = entry.strip_prefix('<').and_then(|value| value.find('>')) else {
        return Err(PaginationError::new(
            PaginationErrorKind::InvalidContinuation,
        ));
    };
    let target = &entry[1..=target_end];
    let parameters = &entry[target_end + 2..];
    let is_next = parameters.split(';').skip(1).any(|parameter| {
        let Some((name, value)) = parameter.trim().split_once('=') else {
            return false;
        };
        name.eq_ignore_ascii_case("rel")
            && value
                .trim()
                .trim_matches('"')
                .split_ascii_whitespace()
                .any(|relation| relation.eq_ignore_ascii_case("next"))
    });
    Ok(is_next.then_some(target))
}

impl<C> CursorPagination<C> {
    /// Creates cursor pagination with an optional initial cursor.
    #[must_use]
    pub const fn new(cursor: Option<C>) -> Self {
        Self { cursor }
    }

    /// Returns the current cursor by reference.
    #[must_use]
    pub const fn cursor(&self) -> Option<&C> {
        self.cursor.as_ref()
    }
}

impl<Page, C> PaginationController<Page> for CursorPagination<C>
where
    Page: HasNextCursor<C> + IntoIterator + Send + 'static,
    Page::Item: Send + 'static,
    C: Clone + Send + 'static,
{
    type Item = Page::Item;

    const DESCRIPTION: PaginationDescription = PaginationDescription::NONE;

    fn advance(
        self,
        page: PageResponse<Page>,
        limits: PageDecisionLimits,
    ) -> Result<PageStep<Self, Self::Item>, PaginationError> {
        let next = page
            .page()
            .next_cursor()
            .cloned()
            .map(|cursor| Self::new(Some(cursor)));
        let items = collect_items(page.into_page(), limits)?;
        Ok(match next {
            Some(next) => PageStep::Continue { items, next },
            None => PageStep::Done { items },
        })
    }
}

fn collect_items<Page>(
    page: Page,
    limits: PageDecisionLimits,
) -> Result<Vec<Page::Item>, PaginationError>
where
    Page: IntoIterator,
{
    let mut items = Vec::new();
    for item in page {
        if items.len() == limits.remaining_items() {
            return Err(PaginationError::new(
                PaginationErrorKind::CollectedItemCeilingExceeded,
            ));
        }
        items.push(item);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct CursorPage {
        items: Vec<u8>,
        next: Option<String>,
    }

    impl IntoIterator for CursorPage {
        type Item = u8;
        type IntoIter = std::vec::IntoIter<u8>;

        fn into_iter(self) -> Self::IntoIter {
            self.items.into_iter()
        }
    }

    impl HasNextCursor<String> for CursorPage {
        fn next_cursor(&self) -> Option<&String> {
            self.next.as_ref()
        }
    }

    #[test]
    fn controllers_expose_exact_initial_state_and_controller_contracts() {
        let offset = OffsetPagination::new(7, NonZeroU32::new(4).expect("nonzero"));
        assert_eq!(offset.offset(), 7);
        assert_eq!(offset.page_size().get(), 4);

        let page = PagePagination::new(
            NonZeroU64::new(9).expect("nonzero"),
            NonZeroU32::new(2).expect("nonzero"),
        );
        assert_eq!(page.page().get(), 9);
        assert_eq!(page.page_size().get(), 2);

        let cursor = CursorPagination::new(None::<String>);
        assert!(cursor.cursor().is_none());

        fn assert_controller<Page, C: PaginationController<Page>>() {}
        assert_controller::<Vec<u8>, OffsetPagination>();
        assert_controller::<Vec<u8>, PagePagination>();
        assert_controller::<CursorPage, CursorPagination<String>>();
    }

    #[test]
    fn link_cursor_resolution_accepts_relative_same_origin_and_rejects_cross_origin() {
        let current = concord_core::contracts::Url::parse(
            "https://catalog.example.test/items?cursor=current",
        )
        .expect("current URL");
        assert_eq!(
            resolve_link_cursor(
                &current,
                ["</items?cursor=next%20page>; rel=\"next\", </items>; rel=prev"],
            )
            .expect("same-origin Link"),
            Some("next page".to_owned())
        );

        let error = resolve_link_cursor(
            &current,
            ["<https://attacker.example/items?cursor=stolen>; rel=next"],
        )
        .expect_err("cross-origin Link");
        assert_eq!(error.kind(), PaginationErrorKind::CrossOriginContinuation);
    }
}
