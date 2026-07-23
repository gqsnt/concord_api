#![forbid(unsafe_code)]

use concord::PaginatedRequest;
use concord::api;
use concord::prelude::PageLimit;

api! {
    client PaginationApi {
        base "https://pagination.example.test/";
    }

    GET Offset(offset: u64, limit: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "offset" = ep.offset; "limit" = ep.limit; }
        paginate concord::advanced::OffsetPagination {
            offset: bind ep.offset,
            page_size: bind ep.limit,
        }
    }
    GET Page(page: std::num::NonZeroU64, per_page: std::num::NonZeroU32) -> Json<Vec<u32>> {
        query { "page" = ep.page; "per_page" = ep.per_page; }
        paginate concord::advanced::PagePagination {
            page: bind ep.page,
            page_size: bind ep.per_page,
        }
    }
    GET Cursor(cursor: Option<String>) -> concord_examples::ExamplePageCodec {
        query { "cursor" = ep.cursor; }
        paginate concord::advanced::CursorPagination<String> {
            cursor: bind ep.cursor,
        }
    }
    GET Custom(cursor: Option<String>) -> concord_examples::ExamplePageCodec {
        query { "cursor" = ep.cursor; }
        paginate concord_examples::ExampleController {
            cursor: bind ep.cursor,
        }
    }
}

fn main() -> Result<(), concord::prelude::ClientBuildError> {
    use pagination_api::prelude::*;

    let client = PaginationApi::builder().build()?;
    let _: PaginatedRequest<_> = client
        .offset(
            0,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client
        .page(
            std::num::NonZeroU64::MIN,
            std::num::NonZeroU32::new(50).expect("positive page size"),
        )
        .paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client.cursor().paginate(PageLimit::Take(2));
    let _: PaginatedRequest<_> = client.custom().paginate(PageLimit::Take(2));
    Ok(())
}
