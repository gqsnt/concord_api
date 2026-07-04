use concord_macros::api;

api! {
    client ReservedNoContentRequestInvalidApi { base "https://example.com" }

    POST Create(body: NoContent)
        path ["create"]
        -> Json<()>
}

fn main() {}
