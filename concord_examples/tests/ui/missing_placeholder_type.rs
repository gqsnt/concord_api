use concord_macros::api;

api! {
    client UiMissingType {
        base https "example.com"
    }

    GET One(id)
        path ["x", id]
    -> Json<()>
}

fn main() {}
