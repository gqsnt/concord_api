use concord_macros::api;

api! {
    client ReservedEndpointApi { base "https://example.com" }

    GET Request
        as request
        path ["request"]
        -> Json<String>
}

fn main() {}
