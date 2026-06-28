use concord_macros::api;

api! {
    client WebSocketHttpPostResponseApi {
        base "https://example.com"
    }

    POST Connect
        path ["connect"]
        -> WebSocket<String, String>
}

fn main() {}
