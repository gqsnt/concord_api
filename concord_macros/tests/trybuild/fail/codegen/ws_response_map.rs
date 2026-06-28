use concord_macros::api;

api! {
    client WsResponseMapApi {
        base "https://example.com"
    }

    WS Connect
        path ["connect"]
        -> WebSocket<String, String>
        map String {
            String::new()
        }
}

fn main() {}
