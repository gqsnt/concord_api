use concord_macros::api;

api! {
    client ReservedBytesWrongArityApi { base "https://example.com" }

    GET Ping
        path ["ping"]
        -> Bytes<u8>
}

fn main() {}
