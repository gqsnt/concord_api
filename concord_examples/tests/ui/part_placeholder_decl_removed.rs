use concord_macros::api;

api! {
    client UiPartPlaceholderRemoved {
        base https "example.com"
    }

    GET One(id: String) -> Json<()> {
        query {
            "q" = part["id:", {id: String}]
        }
    }
}

fn main() {}
