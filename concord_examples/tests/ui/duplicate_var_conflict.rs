use concord_macros::api;

api! {
    client UiDup {
        base https "example.com"
    }

    GET A(id: u32, id: String)
        path ["x", id]
    -> Json<()>
}

fn main() {}
