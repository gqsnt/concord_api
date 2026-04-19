use concord_macros::api;

api! {
    client UiEndpointRouteFirstRemoved {
        scheme: https,
        host: "example.com",
    }

    GET Ping "health" -> Json<()>;
}

fn main() {}
