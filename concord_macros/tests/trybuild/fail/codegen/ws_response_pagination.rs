use concord_macros::api;

api! {
    client WsResponsePaginationApi {
        base "https://example.com"
    }

    WS Connect
        path ["connect"]
        paginate CursorPagination
        -> WebSocket<String, String>
}

fn main() {}
