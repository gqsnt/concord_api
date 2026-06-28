use concord_macros::api;

api! {
    client WsRequestBodyInvalidApi {
        base "https://example.com"
    }

    WS Connect(body: Json<String>)
        path ["connect"]
        -> WebSocket<String, String>
}

fn main() {}
