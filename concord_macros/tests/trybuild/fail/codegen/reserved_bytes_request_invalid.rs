use concord_macros::api;

api! {
    client ReservedBytesRequestInvalidApi {
        base "https://example.com"
    }

    POST Create(body: Bytes)
        path ["create"]
        -> Json<()>
}

fn main() {}
