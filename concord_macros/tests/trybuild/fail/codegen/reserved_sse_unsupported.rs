use concord_macros::api;

api! {
    client ReservedSseUnsupportedApi { base "https://example.com" }

    GET Events
        path ["events"]
        -> Sse<String>
}

fn main() {}
