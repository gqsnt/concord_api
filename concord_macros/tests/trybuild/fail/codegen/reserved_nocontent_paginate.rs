use concord_macros::api;

api! {
    client ReservedNoContentPaginateApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        paginate OffsetLimitPagination {
            offset = 0,
            limit = 10
        }
        -> NoContent
}

fn main() {}
