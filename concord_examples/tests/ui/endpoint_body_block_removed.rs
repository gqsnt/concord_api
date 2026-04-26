use concord_macros::api;

#[derive(serde::Serialize)]
struct Payload;

api! {
    client UiEndpointBodyBlockRemoved {
        base https "example.com"
    }

    POST Create
    -> Json<()>
    {
        path ["create"]
        body Json<Payload>
    }
}

fn main() {}
