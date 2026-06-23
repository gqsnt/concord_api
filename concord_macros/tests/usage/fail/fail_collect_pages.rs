use concord_core::prelude::*;
use concord_macros::api;
use self::usage_collect_pages_api::UsageCollectPagesApi;

api! {
    client UsageCollectPagesApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 20)
        path ["items"]
        query {
            start
            count
        }
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Json<Vec<String>>
}

async fn bad_usage(api: UsageCollectPagesApi) -> Result<(), ApiClientError> {
    let _ = api
        .list()
        .paginate(PaginationTermination::hard_page_cap(10))
        .collect_pages()
        .await?;
    Ok(())
}

fn main() {}
