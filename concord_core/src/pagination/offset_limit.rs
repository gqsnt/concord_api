use crate::endpoint::PaginationPlan;
use crate::pagination::Stop;
use std::borrow::Cow;

/// Offset/limit pagination (offset starts at 0 by default).
///
/// This is the single "engine" for all offset-based APIs:
/// - you bind `offset` and `limit` to endpoint params via `paginate { offset: start, limit: count }`
/// - codegen can hint the effective query keys so this controller remains opaque to codegen.
#[derive(Clone, Debug)]
pub struct OffsetLimitPagination {
    pub stop: Stop,
    /// Query key used for the offset (ex: "offset", "start", "skip").
    pub offset_key: Cow<'static, str>,
    /// Query key used for the limit (ex: "limit", "count", "top").
    pub limit_key: Cow<'static, str>,
    /// Initial offset value.
    pub offset: u64,
    /// Page size / limit (must be > 0).
    pub limit: u64,
    pub stop_on_short_page: bool,
}

impl Default for OffsetLimitPagination {
    fn default() -> Self {
        Self {
            stop: Stop::default(),
            offset_key: Cow::from("offset"),
            limit_key: Cow::from("limit"),
            offset: 0,
            limit: 20,
            stop_on_short_page: true,
        }
    }
}

impl From<OffsetLimitPagination> for PaginationPlan {
    fn from(value: OffsetLimitPagination) -> Self {
        Self::OffsetLimit {
            offset_key: value.offset_key.into_owned(),
            limit_key: value.limit_key.into_owned(),
            offset: value.offset,
            limit: value.limit,
            stop_on_short_page: value.stop_on_short_page,
            stop: value.stop,
        }
    }
}
