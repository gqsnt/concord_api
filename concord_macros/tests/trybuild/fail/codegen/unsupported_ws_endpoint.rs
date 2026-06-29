use concord_macros::api;

api! {
    client WsRemovedApi {
        base "https://example.com"
    }

    WS Connect
        path ["ws"]
        -> Json<String>
}

fn main() {}
