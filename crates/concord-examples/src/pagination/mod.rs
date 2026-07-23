//! Built-in and custom sequential, bounded pagination controllers.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner:
//! `docs/guide/06-pagination-and-rate-limits.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;

pub mod cursor;
pub mod custom;
pub mod offset;
pub mod page_number;

api! {
    client PaginationApi {
        base "http://pagination.example.test/";
    }

    GET offset(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "offset" = ep.offset; "limit" = ep.limit; }
        paginate concord::pagination::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
    GET page(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "page" = ep.page; "per_page" = ep.per_page; }
        paginate concord::pagination::PagePagination {
            page: bind ep.page,
            page_size: bind ep.per_page,
        }
    }
    GET cursor(
        cursor: Option<String>
    ) -> crate::pagination::custom::ExamplePageCodec {
        query { "cursor" = ep.cursor; }
        paginate concord::pagination::CursorPagination<String> {
            cursor: bind ep.cursor,
        }
    }
    GET custom(
        cursor: Option<String>
    ) -> crate::pagination::custom::ExamplePageCodec {
        query { "cursor" = ep.cursor; }
        paginate crate::pagination::custom::ExampleController {
            cursor: bind ep.cursor,
        }
    }
}
