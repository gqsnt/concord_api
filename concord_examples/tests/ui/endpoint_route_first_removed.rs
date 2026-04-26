use concord_macros::api;

api! {
    client UiEndpointRouteFirstRemoved {
        base https "example.com"
    }

    GET Ping "health" -> Json<()>;
}

fn main() {}
