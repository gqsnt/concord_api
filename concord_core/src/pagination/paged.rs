use crate::endpoint::PaginationPlan;
use std::borrow::Cow;

/// Page/per_page pagination (page starts at 1 by default).
#[derive(Clone, Debug)]
pub struct PagedPagination {
    /// Query key for the page number (ex: "page", "_page", "currentPage").
    pub page_key: Cow<'static, str>,
    /// Query key for page size (ex: "per_page", "_limit", "pageSize").
    pub per_page_key: Cow<'static, str>,

    /// Initial page number.
    pub page: u64,
    /// Page size (must be > 0).
    pub per_page: u64,
}

impl Default for PagedPagination {
    fn default() -> Self {
        Self {
            page_key: Cow::from("page"),
            per_page_key: Cow::from("per_page"),
            page: 1,
            per_page: 20,
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
        }
    }
}
