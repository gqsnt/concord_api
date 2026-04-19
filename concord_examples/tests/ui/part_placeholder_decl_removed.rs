use concord_macros::api;

api! {
    client UiPartPlaceholderRemoved {
        scheme: https,
        host: "example.com",
    }

    GET One(id: String) -> Json<()> {
        query {
            "q" = part["id:", {id: String}]
        }
    }
}

fn main() {}
