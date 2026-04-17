use concord_macros::api;

api! {
    client UiRemovedPathLayer {
        scheme: https,
        host: "example.com",
    }

    path users {
        GET One {
            path["x"]
            -> Json<()>;
        }
    }
}

fn main() {}
