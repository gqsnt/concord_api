use concord_macros::api;

api! {
    client ReservedWebSocketRequestInvalidApi {
        base "https://example.com"
    }

    POST Send(body: WebSocket<String, String>)
        path ["send"]
        -> Json<String>
}

fn main() {}
