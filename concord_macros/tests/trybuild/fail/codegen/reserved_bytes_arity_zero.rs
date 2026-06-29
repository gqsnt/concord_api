use concord_macros::api;

api! {
    client ReservedBytesArityZeroApi { base "https://example.com" }

    GET Download
        path ["download"]
        -> Bytes<>
}

fn main() {}
