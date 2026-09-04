//! Built-in and custom sequential, bounded pagination controllers.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner:
//! `docs/pagination.md`.
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
        timeout 30s;
        base "http://pagination.example.test/";


    // ANCHOR: offset_endpoint
    GET offset(offset: u64, limit: std::num::NonZeroU32) -> Vec<u32> via Json {
        query { "offset" = endpoint.offset; "limit" = endpoint.limit; }
        paginate via concord::pagination::OffsetPagination {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
    // ANCHOR_END: offset_endpoint

    // ANCHOR: page_endpoint
    GET page(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Vec<u32> via Json {
        query { "page" = endpoint.page; "per_page" = endpoint.per_page; }
        paginate via concord::pagination::PagePagination {
            page -> endpoint.page;
            page_size -> endpoint.per_page;
        }
    }
    // ANCHOR_END: page_endpoint

    // ANCHOR: post_search_endpoint
    POST search(
        offset: u64,
        limit: std::num::NonZeroU32,
        body filter: crate::pagination::search::SearchFilter via concord::Json,
    ) -> Vec<u32> via Json {
        effect safe;
        path ["search"];
        query { "offset" = endpoint.offset; "limit" = endpoint.limit; }
        paginate via concord::pagination::OffsetPagination {
            offset -> endpoint.offset;
            page_size -> endpoint.limit;
        }
    }
    // ANCHOR_END: post_search_endpoint

    // ANCHOR: cursor_endpoints
    GET cursor(
        cursor?: String
    ) -> crate::pagination::custom::ExamplePage via crate::pagination::custom::ExamplePageCodec {
        query { "cursor" = endpoint.cursor; }
        paginate via concord::pagination::CursorPagination<String> {
            cursor -> endpoint.cursor;
        }
    }
    GET custom(
        cursor?: String
    ) -> crate::pagination::custom::ExamplePage via crate::pagination::custom::ExamplePageCodec {
        query { "cursor" = endpoint.cursor; }
        paginate via crate::pagination::custom::ExampleController {
            cursor -> endpoint.cursor;
        }
    }
    // ANCHOR_END: cursor_endpoints
}}
// ANCHOR_END: pagination_contract
