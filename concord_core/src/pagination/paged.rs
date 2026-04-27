use crate::endpoint::PaginationPlan;
use crate::pagination::Stop;
use std::borrow::Cow;

/// Page/per_page pagination (page starts at 1 by default).
#[derive(Clone, Debug)]
pub struct PagedPagination {
    pub stop: Stop,

    /// Query key for the page number (ex: "page", "_page", "currentPage").
    pub page_key: Cow<'static, str>,
    /// Query key for page size (ex: "per_page", "_limit", "pageSize").
    pub per_page_key: Cow<'static, str>,

    /// Initial page number.
    pub page: u64,
    /// Page size (must be > 0).
    pub per_page: u64,

    /// Optional stop condition: stop when the API returns fewer items than `per_page`.
    /// (Useful for APIs that do not return a total and do not return empty last pages.)
    pub stop_on_short_page: bool,
}

impl Default for PagedPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            page_key: Cow::from("page"),
            per_page_key: Cow::from("per_page"),
            page: 1,
            per_page: 20,
            stop_on_short_page: true,
        }
    }
}

impl From<PagedPagination> for PaginationPlan {
    fn from(value: PagedPagination) -> Self {
        Self::Paged {
            page_key: value.page_key.into_owned(),
            per_page_key: value.per_page_key.into_owned(),
            page: value.page,
            per_page: value.per_page,
            stop_on_short_page: value.stop_on_short_page,
            stop: value.stop,
        }
    }
}
