use concord_core::advanced::OctetStream;
use concord_macros::api;

api! {
    client SseResponsePaginationApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 10)
        path ["events"]
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Sse<OctetStream>
}

fn main() {}
