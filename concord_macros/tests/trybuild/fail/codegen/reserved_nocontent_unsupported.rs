use concord_macros::api;

api! {
    client ReservedNoContentUnsupportedApi { base "https://example.com" }

    GET Ping
        path ["ping"]
        -> NoContent
}

fn main() {}
