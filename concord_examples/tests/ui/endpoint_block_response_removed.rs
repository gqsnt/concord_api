use concord_macros::api;

api! {
    client UiEndpointBlockResponseRemoved {
        scheme: https,
        host: "example.com",
    }

    GET Ping {
        -> Json<()>;
    }
}

fn main() {}
