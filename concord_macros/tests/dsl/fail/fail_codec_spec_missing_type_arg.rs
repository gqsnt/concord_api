use concord_macros::api;

api! {
    client CodecSpecMissingTypeArg {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> Json<>
}

fn main() {}
