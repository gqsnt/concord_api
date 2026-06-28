use concord_macros::api;

api! {
    client WebSocketHttpGetResponseApi {
        base "https://example.com"
    }

    GET Connect
        path ["connect"]
        -> WebSocket<String, String>
}

fn main() {}
