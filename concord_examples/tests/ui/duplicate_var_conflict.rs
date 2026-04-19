use concord_macros::api;

api! {
    client UiDup {
        scheme: https,
        host: "example.com",
    }

    GET A(id: u32, id: String)
    -> Json<()>
    {
        path["x", id]
    }
}

fn main() {}
