#![forbid(unsafe_code)]

use concord_core::PaginatedRequest;
use concord_core::prelude::PageLimit;
use concord_macros::api;

api! {
    client PaginationApi {
        base "https://pagination.example.test/";
        GET Offset(offset: u64, limit: u32) -> Json<Vec<u32>> {
            query { "offset" = ep.offset; "limit" = ep.limit; }
            paginate OffsetPagination { offset = ep.offset, page_size = ep.limit }
        }
        GET Page(page: u64, per_page: u32) -> Json<Vec<u32>> {
            query { "page" = ep.page; "per_page" = ep.per_page; }
            paginate PagePagination { page = ep.page, page_size = ep.per_page }
        }
        GET Cursor(cursor: Option<String>) -> concord_examples::ExamplePageCodec {
            query { "cursor" = ep.cursor; }
            paginate CursorPagination<String> { cursor = ep.cursor }
        }
        GET Custom(cursor: Option<String>) -> concord_examples::ExamplePageCodec {
            paginate custom concord_examples::example_controller(ep.cursor);
        }
    }
}

fn main() -> Result<(), concord_core::prelude::ClientBuildError> {
    use pagination_api::prelude::*;

    let client = PaginationApi::builder().build()?;
    let _: PaginatedRequest<_> = client.offset(0, 50).paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client.page(1, 50).paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client.cursor().paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client.custom().paginate(PageLimit::Take(2));
    Ok(())
}
