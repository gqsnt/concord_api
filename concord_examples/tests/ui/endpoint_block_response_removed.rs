use concord_macros::api;

api! {
    client UiEndpointBlockResponseRemoved {
        base https "example.com"
    }

    GET Ping {
        -> Json<()>;
    }
}

fn main() {}
