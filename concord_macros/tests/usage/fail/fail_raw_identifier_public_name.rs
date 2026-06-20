use concord_macros::api;

api! {
    client RawIdentifierApi { base "https://example.com" }

    GET r#type
        path ["type"]
        -> Json<String>
}

fn main() {}
