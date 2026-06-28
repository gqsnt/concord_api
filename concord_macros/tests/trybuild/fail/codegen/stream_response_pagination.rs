use concord_core::advanced::OctetStream;
use concord_macros::api;

api! {
    client StreamResponsePaginationApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 10)
        path ["items"]
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Stream<OctetStream>
}

fn main() {}
