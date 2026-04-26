use concord_macros::api;

api! {
    client UiEndpointParamsBlockRemoved {
        base https "example.com"
    }

    GET Ping(id: String)
    -> Json<()>
    {
        params {
            other: String
        }
        path ["ping", id]
    }
}

fn main() {}
