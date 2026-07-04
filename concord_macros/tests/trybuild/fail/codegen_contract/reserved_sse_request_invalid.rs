use concord_macros::api;

api! {
    client ReservedSseRequestInvalidApi { base "https://example.com" }

    POST Send(body: Sse<String>)
        path ["send"]
        -> Json<()>
}

fn main() {}
