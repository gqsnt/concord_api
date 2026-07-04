use concord_macros::api;

api! {
    client TraceMethodApi {
        base "https://example.com"
    }

    TRACE Ping
        path ["ping"]
        -> Json<String>
}

fn main() {}
