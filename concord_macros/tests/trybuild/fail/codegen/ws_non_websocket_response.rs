use concord_macros::api;

api! {
    client WsNonWebSocketResponseApi {
        base "https://example.com"
    }

    WS Connect
        path ["connect"]
        -> Json<String>
}

fn main() {}
