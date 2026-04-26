use concord_macros::api;

api! {
    client UiRoutePlaceholderRemoved {
        base https "example.com"
    }

    GET One(id: String) -> Json<()> {
        path ["x", {id: String}]
    }
}

fn main() {}
