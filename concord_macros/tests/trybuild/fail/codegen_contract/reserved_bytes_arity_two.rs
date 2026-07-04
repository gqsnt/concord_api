use concord_macros::api;

api! {
    client ReservedBytesArityTwoApi { base "https://example.com" }

    GET Download
        path ["download"]
        -> Bytes<A, B>
}

fn main() {}
