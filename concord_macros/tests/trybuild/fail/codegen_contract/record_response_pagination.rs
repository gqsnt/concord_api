use concord_core::advanced::NdJson;
use concord_macros::api;

api! {
    client RecordResponsePaginationApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 10)
        path ["items"]
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Records<String, NdJson>
}

fn main() {}
