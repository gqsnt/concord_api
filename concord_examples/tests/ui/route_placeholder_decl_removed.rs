use concord_macros::api;

api! {
    client UiRoutePlaceholderRemoved {
        scheme: https,
        host: "example.com",
    }

    GET One(id: String) -> Json<()> {
        path["x", {id: String}]
    }
}

fn main() {}
