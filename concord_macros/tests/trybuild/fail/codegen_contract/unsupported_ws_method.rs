use concord_macros::api;

api! {
    client UnsupportedWsMethodApi {
        base "https://example.com"
    }

    WS Connect
        path ["ws"]
        -> Json<String>
}

fn main() {}
