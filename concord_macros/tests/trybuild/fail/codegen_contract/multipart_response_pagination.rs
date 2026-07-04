use concord_macros::api;
use self::multipart_response_pagination_api::MultipartResponsePaginationApi;

api! {
    client MultipartResponsePaginationApi {
        base "https://example.com"
    }

    GET List(start: u64 = 0, count: u64 = 10)
        path ["items"]
        paginate OffsetLimitPagination {
            offset = start,
            limit = count
        }
        -> Multipart<concord_core::advanced::RawResponsePart>
}

fn main() {}
