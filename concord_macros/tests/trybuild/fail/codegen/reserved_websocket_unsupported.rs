use concord_macros::api;

api! {
    client ReservedWebSocketUnsupportedApi { base "https://example.com" }

    GET Connect
        path ["connect"]
        -> WebSocket<String, String>
}

fn main() {}
