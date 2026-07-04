use concord_macros::api;

api! {
    client ReservedSseArityThreeApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<A, B, C>
}

fn main() {}
