use concord_macros::api;

api! {
    client ReservedWebSocketArityFourApi {
        base "https://example.com"
    }

    WS Connect
        path ["connect"]
        -> WebSocket<A, B, C, D>
}

fn main() {}
