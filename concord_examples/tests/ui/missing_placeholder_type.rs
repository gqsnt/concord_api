use concord_macros::api;

api! {
    client UiMissingType {
        scheme: https,
        host: "example.com",
    }

    GET One {
        params {
            id
        }
        path["x", id]
        -> Json<()>;
    }
}

fn main() {}
