use concord_macros::api;

api! {
    client ReservedBytesUnsupportedApi { base "https://example.com" }

    GET Ping
        path ["ping"]
        -> Bytes
}

fn main() {}
