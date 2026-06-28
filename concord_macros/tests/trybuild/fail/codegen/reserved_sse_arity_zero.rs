use concord_macros::api;

api! {
    client ReservedSseArityZeroApi {
        base "https://example.com"
    }

    GET Events
        path ["events"]
        -> Sse<>
}

fn main() {}
