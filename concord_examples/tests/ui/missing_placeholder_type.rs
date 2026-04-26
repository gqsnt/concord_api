use concord_macros::api;

api! {
    client UiMissingType {
        base https "example.com"
    }

    GET One(id)
    -> Json<()>
    {
        path ["x", id]
    }
}

fn main() {}
