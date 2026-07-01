use concord_macros::api;

api! {
    client CustomEndpointStateUnknownField {
        base "https://example.com"
    }

    GET List(page: u64 = 1)
        paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {
            page = does_not_exist
        }
        -> Json<Vec<String>>
}

fn main() {}
