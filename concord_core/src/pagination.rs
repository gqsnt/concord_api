pub mod cursor;
pub mod offset_limit;
pub mod paged;

pub use cursor::{CursorPagination, HasNextCursor};
pub use offset_limit::OffsetLimitPagination;
pub use paged::PagedPagination;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Control {
    Continue,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ProgressKey {
    U64(u64),
    Str(String),
    Bytes(Vec<u8>),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub enum Stop {
    #[default]
    OnEmpty,
}

#[derive(Copy, Clone, Debug)]
pub struct Caps {
    pub max_pages: u32,
    pub max_items: u64,
    pub detect_loops: bool,
}
impl Default for Caps {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_items: 100_000,
            detect_loops: true,
        }
    }
}
impl Caps {
    #[inline]
    pub fn max_pages(mut self, v: u32) -> Self {
        self.max_pages = v;
        self
    }
    #[inline]
    pub fn max_items(mut self, v: u64) -> Self {
        self.max_items = v;
        self
    }
    #[inline]
    pub fn detect_loops(mut self, v: bool) -> Self {
        self.detect_loops = v;
        self
    }
}

/// Items container returned by a paginated endpoint.
///
/// Contract:
/// - `len()` must reflect the number of items returned in the page.
/// - `into_iter()` must yield exactly those items (no extra allocation required).
pub trait PageItems: Send + 'static {
    type Item: Send + 'static;
    type IntoIter: IntoIterator<Item = Self::Item>;

    fn len(&self) -> usize;

    #[inline]
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn inner_into_iter(self) -> Self::IntoIter;
}
impl<T: Send + 'static> PageItems for Vec<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn len(&self) -> usize {
        Vec::len(self)
    }
    fn inner_into_iter(self) -> Self::IntoIter {
        <Vec<T> as IntoIterator>::into_iter(self)
    }
}
