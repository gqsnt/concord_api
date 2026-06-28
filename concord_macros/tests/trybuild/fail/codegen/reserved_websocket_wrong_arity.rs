use concord_macros::api;

api! {
    client ReservedWebSocketWrongArityApi { base "https://example.com" }

    GET Connect
        path ["connect"]
        -> WebSocket<String>
}

fn main() {}
