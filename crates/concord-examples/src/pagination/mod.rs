//! Built-in and custom sequential, bounded pagination controllers.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner:
//! `docs/features/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.

use concord::api;

pub mod cursor;
pub mod custom;
pub mod offset;
pub mod page_number;
pub mod search;

// ANCHOR: pagination_contract
api! {
    client PaginationApi {
        base "http://pagination.example.test/";
    }

    // ANCHOR: offset_endpoint
    GET offset(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "offset" = ep.offset; "limit" = ep.limit; }
        paginate concord::pagination::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
    // ANCHOR_END: offset_endpoint

    // ANCHOR: page_endpoint
    GET page(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "page" = ep.page; "per_page" = ep.per_page; }
        paginate concord::pagination::PagePagination {
            page: bind ep.page,
            page_size: bind ep.per_page,
        }
    }
    // ANCHOR_END: page_endpoint

    // ANCHOR: post_search_endpoint
    POST search(
        offset: u64,
        limit: std::num::NonZeroU32,
        #[body] filter: Json<crate::pagination::search::SearchFilter>,
    ) -> Json<Vec<u32>> {
        path ["search"];
        query { "offset" = ep.offset; "limit" = ep.limit; }
        paginate concord::pagination::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
    // ANCHOR_END: post_search_endpoint

    // ANCHOR: cursor_endpoints
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
    // ANCHOR_END: cursor_endpoints
}
// ANCHOR_END: pagination_contract
