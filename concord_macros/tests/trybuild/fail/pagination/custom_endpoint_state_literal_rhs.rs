use concord_macros::api;

api! {
    client CustomEndpointStateLiteral {
        base "https://example.com"
    }

    GET List(page: u64 = 1)
        paginate endpoint_state HeaderPagePagination bindings HeaderPageBindings {
            page = 1
        }
        -> Json<Vec<String>>
}

fn main() {}
