use concord_macros::api;

api! {
    client UiRemovedFmt {
        scheme: https,
        host: "example.com",
    }

    GET One {
        params { id: String }
        path[fmt["u-", id]]
        -> Json<()>;
    }
}

fn main() {}
