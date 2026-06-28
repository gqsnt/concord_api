use concord_macros::api;

api! {
    client ReservedWebSocketArityZeroApi {
        base "https://example.com"
    }

    GET Connect
        path ["connect"]
        -> WebSocket<>
}

fn main() {}
