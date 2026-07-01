use concord_macros::api;

api! {
    client CustomEndpointStateMissingBindingsType {
        base "https://example.com"
    }

    GET List(page: u64 = 1)
        paginate endpoint_state HeaderPagePagination {
            page = page
        }
        -> Json<Vec<String>>
}

fn main() {}
