//! Built-in and custom sequential, bounded pagination controllers.
//!
//! Required feature: `json`. Mode: construction and deterministic local
//! multi-page execution. Documentation owner:
//! `docs/pagination.md`.
//! Proving tests: `tests/construction.rs` and `tests/local_runtime.rs`.
//!
//! Every controller is initialized by a public `init` type and maps its whole
//! cursor value onto one endpoint slot (`cursor -> slot`). Ordinary `await`
//! returns one page; `collect_prefix`, `collect_all` and `pages` are the
//! explicit traversal terminals.

use std::num::{NonZeroU32, NonZeroU64};

use concord::pagination::{
    CursorInit, CursorPagination, OffsetInit, OffsetPagination, PageInit, PagePagination,
};
use concord::{Json, api};

use self::custom::{ExampleController, ExamplePage, ExamplePageCodec, ExampleStart};
use self::search::SearchFilter;

pub mod cursor;
pub mod custom;
pub mod envelope;
pub mod offset;
pub mod page_number;
pub mod search;

// ANCHOR: pagination_contract
api! {
    client PaginationApi {
        base "http://pagination.example.test/";
        timeout 30s;

        // ANCHOR: offset_endpoint
        GET offset(query offset: u64, query limit: NonZeroU32) -> Vec<u32> via Json {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
        // ANCHOR_END: offset_endpoint

        // ANCHOR: page_endpoint
        GET page(query page: NonZeroU64, query per_page: NonZeroU32) -> Vec<u32> via Json {
            paginate via PagePagination {
                init PageInit { page: page, page_size: per_page };
                cursor -> page;
            }
        }
        // ANCHOR_END: page_endpoint

        // ANCHOR: post_search_endpoint
        POST search(
            query offset: u64,
            query limit: NonZeroU32,
            body filter: SearchFilter via Json,
        ) -> Vec<u32> via Json {
            effect safe;
            path "search";
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: limit };
                cursor -> offset;
            }
        }
        // ANCHOR_END: post_search_endpoint

        // ANCHOR: cursor_endpoints
        GET cursor(query cursor?: String) -> ExamplePage via ExamplePageCodec {
            paginate via CursorPagination<String> {
                init CursorInit { cursor: cursor };
                cursor -> cursor;
            }
        }

        GET custom(query cursor?: String) -> ExamplePage via ExamplePageCodec {
            paginate via ExampleController {
                init ExampleStart { cursor: cursor };
                cursor -> cursor;
            }
        }
        // ANCHOR_END: cursor_endpoints
    }
}
// ANCHOR_END: pagination_contract
