use concord_macros::api;

api! {
    client UiRemovedPrefixLayer {
        scheme: https,
        host: "example.com",
    }

    prefix api {
        GET One {
            path["x"]
            -> Json<()>;
        }
    }
}

fn main() {}
