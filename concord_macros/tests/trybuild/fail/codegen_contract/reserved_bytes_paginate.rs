use concord_macros::api;

api! {
    client ReservedBytesPaginateApi {
        base "https://example.com"
    }

    GET Download
        path ["download"]
        -> Bytes
        paginate OffsetLimitPagination {
            offset = 0,
            limit = 10
        }
}

fn main() {}
