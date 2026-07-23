//! Built-in policies for core's generic sequential pagination engine.

use std::num::{NonZeroU32, NonZeroU64, NonZeroUsize};

use concord_core::pagination::{PaginationController, PaginationError, PaginationErrorKind};

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

impl<Page> PaginationController<Page> for OffsetPagination {
    fn next(self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let offset = self
            .offset
            .checked_add(u64::from(self.page_size.get()))
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            offset,
            page_size: self.page_size,
        }))
    }

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        Some(
            NonZeroUsize::new(
                usize::try_from(self.page_size.get())
                    .expect("u32 page sizes fit the supported usize targets"),
            )
            .expect("the stored page size is nonzero"),
        )
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

impl<Page> PaginationController<Page> for PagePagination {
    fn next(self, _page: &Page) -> Result<Option<Self>, PaginationError> {
        let page = self
            .page
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or_else(|| PaginationError::new(PaginationErrorKind::Overflow))?;
        Ok(Some(Self {
            page,
            page_size: self.page_size,
        }))
    }

    fn short_page_size(&self) -> Option<NonZeroUsize> {
        Some(
            NonZeroUsize::new(
                usize::try_from(self.page_size.get())
                    .expect("u32 page sizes fit the supported usize targets"),
            )
            .expect("the stored page size is nonzero"),
        )
    }
}

/// Typed cursor pagination state.
#[derive(Clone, Eq, PartialEq)]
pub struct CursorPagination<C> {
    /// Current optional cursor value.
    pub cursor: Option<C>,
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
    Page: HasNextCursor<C>,
    C: Clone + Send + 'static,
{
    fn next(self, page: &Page) -> Result<Option<Self>, PaginationError> {
        Ok(page
            .next_cursor()
            .cloned()
            .map(|cursor| Self::new(Some(cursor))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct CursorPage {
        next: Option<String>,
    }

    impl HasNextCursor<String> for CursorPage {
        fn next_cursor(&self) -> Option<&String> {
            self.next.as_ref()
        }
    }

    #[test]
    fn controllers_progress_checked_and_preserve_absent_vs_empty_cursor() {
        let offset = OffsetPagination::new(7, NonZeroU32::new(4).expect("nonzero"));
        assert_eq!(
            <OffsetPagination as PaginationController<()>>::short_page_size(&offset),
            NonZeroUsize::new(4)
        );
        let next = <OffsetPagination as PaginationController<()>>::next(offset, &())
            .expect("offset next")
            .expect("offset continues");
        assert_eq!(next.offset(), 11);
        assert_eq!(next.page_size().get(), 4);
        let overflow = OffsetPagination::new(u64::MAX, NonZeroU32::MIN);
        let Err(error) = <OffsetPagination as PaginationController<()>>::next(overflow, &()) else {
            panic!("offset overflow must fail");
        };
        assert_eq!(error.kind(), PaginationErrorKind::Overflow);

        let page = PagePagination::new(
            NonZeroU64::new(9).expect("nonzero"),
            NonZeroU32::new(2).expect("nonzero"),
        );
        assert_eq!(
            <PagePagination as PaginationController<()>>::short_page_size(&page),
            NonZeroUsize::new(2)
        );
        let next = <PagePagination as PaginationController<()>>::next(page, &())
            .expect("page next")
            .expect("page continues");
        assert_eq!(next.page().get(), 10);
        assert_eq!(next.page_size().get(), 2);
        let overflow =
            PagePagination::new(NonZeroU64::new(u64::MAX).expect("nonzero"), NonZeroU32::MIN);
        let Err(error) = <PagePagination as PaginationController<()>>::next(overflow, &()) else {
            panic!("page overflow must fail");
        };
        assert_eq!(error.kind(), PaginationErrorKind::Overflow);

        let cursor = CursorPagination::new(None::<String>);
        let empty = cursor
            .next(&CursorPage {
                next: Some(String::new()),
            })
            .expect("cursor next")
            .expect("present empty cursor continues");
        assert_eq!(empty.cursor().map(String::as_str), Some(""));
        assert!(
            empty
                .next(&CursorPage { next: None })
                .expect("absent cursor")
                .is_none()
        );
    }
}
