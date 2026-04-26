use concord_macros::api;

api! {
    client UiUnknownVars {
        base https "example.com"
        headers {
            "x" = vars.missing // ERROR: unknown vars entry
        }
    }
}

fn main() {}
