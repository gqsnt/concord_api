use concord_macros::api;

api! {
    client UiUnknownVars {
        scheme: https,
        host: "example.com",
        headers {
            "x" = vars.missing // ERROR: unknown vars entry
        }
    }
}

fn main() {}
