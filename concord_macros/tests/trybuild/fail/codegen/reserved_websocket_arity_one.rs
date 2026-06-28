use concord_macros::api;

api! {
    client ReservedWebSocketArityOneApi {
        base "https://example.com"
    }

    WS Connect
        path ["connect"]
        -> WebSocket<A>
}

fn main() {}
