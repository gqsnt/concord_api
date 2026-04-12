use concord_macros::api;

api! {
    client UiDup {
        scheme: https,
        host: "example.com",
    }

    GET A {
        params {
            id: u32,
            id: String
        }
        path["x", id]
        -> Json<()>;
    }
}

fn main() {}
