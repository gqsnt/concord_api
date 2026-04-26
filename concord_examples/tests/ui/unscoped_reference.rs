use concord_macros::api;

api! {
    client UiUnscoped {
        base https "example.com"
        headers {
            "x" = token // ERROR: unscoped
        }
    }
}

fn main() {}
